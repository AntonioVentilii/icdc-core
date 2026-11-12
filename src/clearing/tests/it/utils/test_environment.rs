use std::{collections::BTreeMap, sync::Arc};

use candid::{encode_one, CandidType, Nat, Principal};
use clearing::types::state::Config;
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::Deserialize;
use shared::types::{
    minter::{Config as MinterConfig, MinterArg},
    Asset, BalanceDomain, CollateralAssetConfig,
};

use crate::utils::{
    constants::{CKUSDC_LEDGER, ICP_LEDGER, VUSD_ASSET_ID, VUSD_LEDGER},
    mock::*,
    pic_canister::{PicCanister, PicCanisterBuilder, PicCanisterTrait as _},
    trade_helper::TradeHelperTrait as _,
};

#[derive(CandidType, Deserialize, Clone, Debug)]
pub enum MetadataValue {
    Nat(Nat),
    Int(i128),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct FeatureFlags {
    pub icrc2: bool,
}

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct ICRC1Account {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct LedgerInitArg {
    pub token_symbol: String,
    pub token_name: String,
    pub transfer_fee: Nat,
    pub decimals: Option<u8>,
    pub metadata: Vec<(String, MetadataValue)>,
    pub feature_flags: Option<FeatureFlags>,
    pub minting_account: ICRC1Account,
    pub initial_balances: Vec<(ICRC1Account, Nat)>,
    pub archive_options: ArchiveOptions,
}

#[derive(CandidType, Deserialize, Clone, Debug)]
pub enum LedgerArg {
    Init(LedgerInitArg),
    Upgrade(Option<LedgerUpgradeArg>),
}

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct LedgerUpgradeArg {
    pub token_symbol: Option<String>,
    pub token_name: Option<String>,
    pub transfer_fee: Option<Nat>,
    pub decimals: Option<u8>,
    pub metadata: Option<Vec<(String, MetadataValue)>>,
    pub feature_flags: Option<FeatureFlags>,
}

#[derive(CandidType, Deserialize, Debug, Clone)]
pub struct ArchiveOptions {
    pub num_blocks_to_archive: u64,
    pub trigger_threshold: u64,
    pub controller_id: Principal,
    pub cycles_for_archive_creation: Option<u64>,
}

pub struct TestSetup {
    pub pic: Arc<PocketIc>,
    pub clearing: PicCanister,
    pub registry: PicCanister,
    pub minter: PicCanister,
    pub ledgers: BTreeMap<String, PicCanister>,
    pub user: Principal,
    pub controller: Principal,
}

/// Selects which non-essential ledger canisters to deploy in the test
/// environment. Each additional ledger requires extra canister installs
/// (and, for ICP, the NNS subnet), so tests should opt in to only what
/// they need.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LedgerSet {
    /// `vUSD` ledger only.
    None,
    /// `vUSD` + `ICP` ledgers (requires NNS subnet).
    Icp,
    /// `vUSD` + `ICP` + `ckUSDC` ledgers (requires NNS subnet).
    All,
}

impl LedgerSet {
    fn needs_nns(self) -> bool {
        matches!(self, Self::Icp | Self::All)
    }

    fn deploys_icp(self) -> bool {
        matches!(self, Self::Icp | Self::All)
    }

    fn deploys_ckusdc(self) -> bool {
        matches!(self, Self::All)
    }
}

impl Default for TestSetup {
    /// Minimal fixture: a single application subnet hosting the three
    /// application canisters (`clearing`, `registry`, `minter`) plus the
    /// `vUSD` ledger. Tests that need `ICP` or `ckUSDC` collateral
    /// ledgers should use [`TestSetup::with_icp`] or
    /// [`TestSetup::with_all_ledgers`] instead.
    fn default() -> Self {
        Self::build(LedgerSet::None)
    }
}

impl TestSetup {
    /// Fixture with `vUSD` + `ICP` ledgers. Adds the NNS subnet because
    /// the ICP ledger uses a pinned NNS canister ID.
    pub fn with_icp() -> Self {
        Self::build(LedgerSet::Icp)
    }

    /// Fixture with all supported collateral ledgers (`vUSD`, `ICP`,
    /// `ckUSDC`). Adds the NNS subnet for the ICP ledger.
    pub fn with_all_ledgers() -> Self {
        Self::build(LedgerSet::All)
    }

    fn build(ledgers: LedgerSet) -> Self {
        let mut builder = PocketIcBuilder::new().with_application_subnet();
        if ledgers.needs_nns() {
            builder = builder.with_nns_subnet();
        }
        let pic = Arc::new(builder.build());

        let controller = Principal::from_text(CONTROLLER).unwrap();
        let user = Principal::from_text(NON_CONTROLLER).unwrap();

        let registry = PicCanisterBuilder::new("registry")
            .with_controllers(vec![controller])
            .deploy_to(&pic.clone());

        let internal_ledger_id = Principal::from_text(VUSD_LEDGER).unwrap();
        let clearing_config = Config {
            insurance_fund_fee_ratio: 10,
            protocol_fee_ratio: 5,
            evm_rpc: Principal::anonymous(),
            signer_canister: Principal::anonymous(),
            internal_ledger: CollateralAssetConfig {
                asset_id: VUSD_ASSET_ID.to_owned(),
                asset: Asset::Icrc(internal_ledger_id),
                symbol: VUSD_ASSET_ID.to_owned(),
                decimals: 8,
                is_enabled: true,
                oracle_id: None,
                allowed_balance_domains: vec![BalanceDomain::Settlement, BalanceDomain::Playground],
            },
            version: 1,
        };

        let clearing = PicCanisterBuilder::new("clearing")
            .with_controllers(vec![controller])
            .with_arg(encode_one(clearing_config).unwrap())
            .deploy_to(&pic.clone());

        let vusd_ledger = deploy_vusd_ledger(&pic, controller, clearing.canister_id());

        let minter_config = MinterConfig {
            ledger_canister: internal_ledger_id,
            authorized_callers: vec![controller, clearing.canister_id()],
        };

        let minter = PicCanisterBuilder::new("minter")
            .with_arg(encode_one(MinterArg::Init(minter_config)).unwrap())
            .with_controllers(vec![controller])
            .deploy_to(&pic.clone());

        clearing
            .update::<(), _>(
                controller,
                "set_registry_canister",
                (registry.canister_id(),),
            )
            .expect("Failed to link registry");

        let mut ledger_map = BTreeMap::new();
        ledger_map.insert(VUSD_ASSET_ID.to_owned(), vusd_ledger);

        if ledgers.deploys_icp() {
            let icp_ledger = deploy_icp_ledger(&pic, controller);
            ledger_map.insert("ICP".to_owned(), icp_ledger);
        }

        if ledgers.deploys_ckusdc() {
            let ckusdc_ledger = deploy_ckusdc_ledger(&pic, controller);
            ledger_map.insert("ckUSDC".to_owned(), ckusdc_ledger);
        }

        let setup = Self {
            pic,
            clearing,
            registry,
            minter,
            ledgers: ledger_map,
            user,
            controller,
        };

        setup.setup_vusd();
        if ledgers.deploys_icp() {
            setup.setup_icp();
        }
        if ledgers.deploys_ckusdc() {
            setup.setup_ckusdc();
        }

        setup
    }
}

fn standard_initial_balances() -> Vec<(ICRC1Account, Nat)> {
    (54_u8..=58)
        .map(|id| {
            (
                ICRC1Account {
                    owner: test_user(id),
                    subaccount: None,
                },
                Nat::from(1_000_000_000_000_000_u64),
            )
        })
        .collect()
}

fn standard_archive_options(controller: Principal) -> ArchiveOptions {
    ArchiveOptions {
        num_blocks_to_archive: 1000,
        trigger_threshold: 2000,
        controller_id: controller,
        cycles_for_archive_creation: None,
    }
}

fn deploy_vusd_ledger(
    pic: &Arc<PocketIc>,
    controller: Principal,
    clearing_id: Principal,
) -> PicCanister {
    let ledger_id = Principal::from_text(VUSD_LEDGER).unwrap();
    let ledger_arg = LedgerArg::Init(LedgerInitArg {
        token_symbol: VUSD_ASSET_ID.to_owned(),
        token_name: "Virtual USD".to_owned(),
        transfer_fee: Nat::from(100_000_u64),
        decimals: Some(8),
        metadata: vec![],
        feature_flags: Some(FeatureFlags { icrc2: true }),
        minting_account: ICRC1Account {
            owner: controller,
            subaccount: None,
        },
        initial_balances: standard_initial_balances(),
        archive_options: standard_archive_options(controller),
    });

    let mut builder = PicCanisterBuilder::new("ledger");
    builder.wasm_path = PicCanister::workspace_dir()
        .join("target/ic/ledger.wasm")
        .to_string_lossy()
        .to_string();
    builder.arg = encode_one(ledger_arg).unwrap();
    builder.controllers = Some(vec![controller, clearing_id]);
    builder.canister_id = Some(ledger_id);
    builder.deploy_to(&pic.clone())
}

fn deploy_icp_ledger(pic: &Arc<PocketIc>, controller: Principal) -> PicCanister {
    let icp_ledger_id = Principal::from_text(ICP_LEDGER).unwrap();
    let icp_ledger_arg = LedgerArg::Init(LedgerInitArg {
        token_symbol: "ICP".to_owned(),
        token_name: "Internet Computer".to_owned(),
        transfer_fee: Nat::from(10_000_u64),
        decimals: Some(8),
        metadata: vec![],
        feature_flags: Some(FeatureFlags { icrc2: true }),
        minting_account: ICRC1Account {
            owner: controller,
            subaccount: None,
        },
        initial_balances: standard_initial_balances(),
        archive_options: standard_archive_options(controller),
    });

    let mut builder = PicCanisterBuilder::new("icp_ledger");
    builder.wasm_path = PicCanister::workspace_dir()
        .join("target/ic/ledger.wasm")
        .to_string_lossy()
        .to_string();
    builder.arg = encode_one(icp_ledger_arg).unwrap();
    builder.controllers = Some(vec![controller]);
    builder.canister_id = Some(icp_ledger_id);
    builder.deploy_to(&pic.clone())
}

fn deploy_ckusdc_ledger(pic: &Arc<PocketIc>, controller: Principal) -> PicCanister {
    let ckusdc_ledger_id = Principal::from_text(CKUSDC_LEDGER).unwrap();
    let ckusdc_ledger_arg = LedgerArg::Init(LedgerInitArg {
        token_symbol: "ckUSDC".to_owned(),
        token_name: "Chain-key USDC".to_owned(),
        transfer_fee: Nat::from(10_u64),
        decimals: Some(6),
        metadata: vec![],
        feature_flags: Some(FeatureFlags { icrc2: true }),
        minting_account: ICRC1Account {
            owner: controller,
            subaccount: None,
        },
        initial_balances: vec![(
            ICRC1Account {
                owner: test_user(54),
                subaccount: None,
            },
            Nat::from(1_000_000_000_u64),
        )],
        archive_options: standard_archive_options(controller),
    });

    let mut builder = PicCanisterBuilder::new("ckusdc_ledger");
    builder.wasm_path = PicCanister::workspace_dir()
        .join("target/ic/ledger.wasm")
        .to_string_lossy()
        .to_string();
    builder.arg = encode_one(ckusdc_ledger_arg).unwrap();
    builder.controllers = Some(vec![controller]);
    builder.canister_id = Some(ckusdc_ledger_id);
    builder.deploy_to(&pic.clone())
}

pub fn test_user(id: u8) -> Principal {
    Principal::from_slice(&[id, 1, 1])
}
