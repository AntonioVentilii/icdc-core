use core::str::from_utf8;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use candid::{
    decode_one, encode_args, encode_one, utils::ArgumentEncoder, CandidType, Deserialize, Principal,
};
use pocket_ic::PocketIc;

/// Common methods for interacting with a canister using `PocketIc`.
pub trait PicCanisterTrait {
    /// A shared `PocketIc` instance.
    ///
    /// Note: `PocketIc` uses interior mutability for query and update calls.  No external mut
    /// annotation or locks appear to be necessary.
    fn pic(&self) -> Arc<PocketIc>;

    /// The ID of this canister.
    fn canister_id(&self) -> Principal;

    /// Makes an update call to the canister.
    fn update<T, A>(&self, caller: Principal, method: &str, arg: A) -> Result<T, String>
    where
        T: for<'a> Deserialize<'a> + CandidType,
        A: ArgumentEncoder,
    {
        let bytes = self
            .pic()
            .update_call(
                self.canister_id(),
                caller,
                method,
                encode_args(arg).unwrap(),
            )
            .map_err(|e| {
                format!(
                    "Update call error. RejectionCode: {:?}, Error: {}",
                    e.reject_code, e.reject_message
                )
            })?;
        decode_one(&bytes).map_err(|e| format!("Decoding failed: {e}"))
    }

    /// Makes a query call to the canister.
    fn query<T, A>(&self, caller: Principal, method: &str, arg: A) -> Result<T, String>
    where
        T: for<'a> Deserialize<'a> + CandidType,
        A: ArgumentEncoder,
    {
        let bytes = self
            .pic()
            .query_call(
                self.canister_id(),
                caller,
                method,
                encode_args(arg).unwrap(),
            )
            .map_err(|e| {
                format!(
                    "Query call error. RejectionCode: {:?}, Error: {}",
                    e.reject_code, e.reject_message
                )
            })?;
        decode_one(&bytes).map_err(|_| "Decoding failed".to_owned())
    }

    fn workspace_dir() -> PathBuf {
        let output = Command::new(env!("CARGO"))
            .arg("locate-project")
            .arg("--workspace")
            .arg("--message-format=plain")
            .output()
            .unwrap()
            .stdout;
        let cargo_path = Path::new(from_utf8(&output).unwrap().trim());
        cargo_path.parent().unwrap().to_path_buf()
    }

    /// The path to a typical Cargo Wasm build.
    fn cargo_wasm_path(name: &str) -> String {
        let workspace_dir = Self::workspace_dir();
        workspace_dir
            .join("target/wasm32-unknown-unknown/release")
            .join(name)
            .with_extension("wasm")
            .to_str()
            .unwrap()
            .to_owned()
    }

    /// The path to the wasm after `dfx deploy`.  Expects the Wasm to be gzipped.
    ///
    /// If not already gzipped, please add this to the canister declaration in `dfx.json`: `"gzip":
    /// true`
    #[expect(dead_code)]
    fn dfx_wasm_path(name: &str) -> String {
        Self::workspace_dir()
            .join(format!(".dfx/local/canisters/{name}/{name}.wasm.gz"))
            .to_str()
            .unwrap()
            .to_owned()
    }
}

/// A typical canister running on `PocketIC`.
pub struct PicCanister {
    pub pic: Arc<PocketIc>,
    pub canister_id: Principal,
}

impl PicCanisterTrait for PicCanister {
    /// The shared `PocketIc` instance.
    fn pic(&self) -> Arc<PocketIc> {
        self.pic.clone()
    }

    /// The ID of this canister.
    fn canister_id(&self) -> Principal {
        self.canister_id
    }
}

/// Canister installer, using the builder pattern, for use in test environmens using `PocketIC`.
///
/// # Example
/// For a default test environment:
/// ```
/// let pic_canister = BackendBuilder::default().deploy();
/// ```
/// To add a backend canister to an existing `PocketIC`:
/// ```
/// let pic = PocketIc::new();
/// let canister_id = BackendBuilder::default().deploy_to(&pic);
/// ```
/// To redeploy an existing canister:
/// ```
/// // First deployment:
/// let (pic, canister_id) = BackendBuilder::default().deploy();
/// // Subsequent deployment:
/// let canister_id = BackendBuilder::default().with_canister(canister_id).deploy_to(&pic);
/// ```
/// To customise the deployment, use the `.with_*` modifiers.  E.g.:
/// ```
/// let (pic, canister_id) = BackendBuilder::default()
///    .with_wasm("path/to/ic_chainfusion_signer.wasm")
///    .with_arg(vec![1, 2, 3])
///    .with_controllers(vec![Principal::from_text("controller").unwrap()])
///    .with_cycles(1_000_000_000_000)
///    .deploy();
/// ```
#[derive(Debug)]
pub struct PicCanisterBuilder {
    /// Canister ID of the backend canister.  If not set, a new canister will be created.
    pub canister_id: Option<Principal>,
    /// Cycles to add to the backend canister.
    pub cycles: u128,
    /// Path to the backend wasm file.
    pub wasm_path: String,
    /// Argument to pass to the backend canister.
    pub arg: Vec<u8>,
    /// Controllers of the backend canister.
    ///
    /// If the list is not specified, controllers will be unchnaged from the `PocketIc` defaults.
    pub controllers: Option<Vec<Principal>>,
}

impl PicCanisterBuilder {
    /// The default number of cycles to add to the backend canister on deployment.
    ///
    /// To override, please use `with_cycles()`.
    pub const DEFAULT_CYCLES: u128 = 2_000_000_000_000;

    /// The default argument to pass to the backend canister:  `()`.
    ///
    /// To override, please use `with_arg()`.
    pub fn default_arg() -> Vec<u8> {
        encode_one(()).unwrap()
    }

    pub fn new(wasm_name: &str) -> Self {
        Self {
            wasm_path: PicCanister::cargo_wasm_path(wasm_name),
            ..Self::default()
        }
    }

    /// Sets a custom argument for the backend canister.
    pub fn with_arg(mut self, arg: Vec<u8>) -> Self {
        self.arg = arg;
        self
    }

    /// Sets custom controllers for the backend canister.
    pub fn with_controllers(mut self, controllers: Vec<Principal>) -> Self {
        self.controllers = Some(controllers);
        self
    }

    /// Reads the backend Wasm bytes from the configured path.
    fn wasm_bytes(&self) -> Vec<u8> {
        fs::read(self.wasm_path.clone())
            .unwrap_or_else(|_| panic!("Could not find the backend wasm: {}", self.wasm_path))
    }

    /// Get or create canister.
    fn canister_id(&mut self, pic: &PocketIc) -> Principal {
        if let Some(canister_id) = self.canister_id {
            canister_id
        } else {
            let canister_id = pic.create_canister();
            self.canister_id = Some(canister_id);
            canister_id
        }
    }

    /// Add cycles to the backend canister.
    fn add_cycles(&mut self, pic: &PocketIc) {
        if self.cycles > 0 {
            let canister_id = self.canister_id(pic);
            pic.add_cycles(canister_id, self.cycles);
        }
    }

    /// Install the backend canister.
    fn install(&mut self, pic: &PocketIc) {
        let wasm_bytes = self.wasm_bytes();
        let canister_id = self.canister_id(pic);
        let arg = self.arg.clone();
        pic.install_canister(canister_id, wasm_bytes, arg, None);
    }

    /// Set controllers of the backend canister.
    fn set_controllers(&mut self, pic: &PocketIc) {
        if let Some(controllers) = self.controllers.clone() {
            let canister_id = self.canister_id(pic);
            pic.set_controllers(canister_id, None, controllers)
                .expect("Test setup error: Failed to set controllers");
        }
    }

    /// Setup the backend canister.
    pub fn deploy_to(mut self, pic: &Arc<PocketIc>) -> PicCanister {
        let canister_id = if let Some(id) = self.canister_id {
            if !pic.canister_exists(id) {
                pic.create_canister_with_id(None, None, id)
                    .expect("Failed to create canister with pre-assigned ID");
            }
            id
        } else {
            pic.create_canister()
        };
        self.canister_id = Some(canister_id);

        self.add_cycles(pic);
        self.install(pic);
        self.set_controllers(pic);
        PicCanister {
            pic: pic.clone(),
            canister_id,
        }
    }
}

impl Default for PicCanisterBuilder {
    fn default() -> Self {
        Self {
            canister_id: None,
            cycles: Self::DEFAULT_CYCLES,
            wasm_path: "unspecified.wasm".to_owned(),
            arg: Self::default_arg(),
            controllers: None,
        }
    }
}
