use std::{collections::BTreeMap, sync::Arc};

use candid::{encode_one, CandidType, Nat, Principal};
use clearing::types::state::Config;
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::Deserialize;
use shared::types::{minter::Config as MinterConfig, Asset, BalanceDomain, CollateralAssetConfig};

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

impl Default for TestSetup {
    fn default() -> Self {
        let pic = Arc::new(
            PocketIcBuilder::new()
                .with_nns_subnet()
                .with_system_subnet()
                .with_application_subnet()
                .build(),
        );

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

        // Deploy Ledger (vUSD)
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
            initial_balances: vec![
                (
                    ICRC1Account {
                        owner: test_user(54),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
                (
                    ICRC1Account {
                        owner: test_user(55),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
                (
                    ICRC1Account {
                        owner: test_user(56),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
                (
                    ICRC1Account {
                        owner: test_user(57),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
                (
                    ICRC1Account {
                        owner: test_user(58),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
            ],
            archive_options: ArchiveOptions {
                num_blocks_to_archive: 1000,
                trigger_threshold: 2000,
                controller_id: controller,
                cycles_for_archive_creation: None,
            },
        });

        let mut ledger_builder = PicCanisterBuilder::new("ledger");
        ledger_builder.wasm_path = PicCanister::workspace_dir()
            .join("target/ic/ledger.wasm")
            .to_string_lossy()
            .to_string();
        ledger_builder.arg = encode_one(ledger_arg).unwrap();
        ledger_builder.controllers = Some(vec![controller, clearing.canister_id()]);
        ledger_builder.canister_id = Some(ledger_id);
        let ledger_can = ledger_builder.deploy_to(&pic.clone());

        // Deploy Ledger (ICP)
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
            initial_balances: vec![
                (
                    ICRC1Account {
                        owner: test_user(54),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
                (
                    ICRC1Account {
                        owner: test_user(55),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
                (
                    ICRC1Account {
                        owner: test_user(56),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
                (
                    ICRC1Account {
                        owner: test_user(57),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
                (
                    ICRC1Account {
                        owner: test_user(58),
                        subaccount: None,
                    },
                    Nat::from(1_000_000_000_000_000_u64),
                ),
            ],
            archive_options: ArchiveOptions {
                num_blocks_to_archive: 1000,
                trigger_threshold: 2000,
                controller_id: controller,
                cycles_for_archive_creation: None,
            },
        });

        let mut icp_ledger_builder = PicCanisterBuilder::new("icp_ledger");
        icp_ledger_builder.wasm_path = PicCanister::workspace_dir()
            .join("target/ic/ledger.wasm") // Using the same ledger.wasm as vUSD
            .to_string_lossy()
            .to_string();
        icp_ledger_builder.arg = encode_one(icp_ledger_arg).unwrap();
        icp_ledger_builder.controllers = Some(vec![controller]);
        icp_ledger_builder.canister_id = Some(icp_ledger_id);
        let icp_ledger_can = icp_ledger_builder.deploy_to(&pic.clone());

        // Deploy Ledger (ckUSDC)
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
            archive_options: ArchiveOptions {
                num_blocks_to_archive: 1000,
                trigger_threshold: 2000,
                controller_id: controller,
                cycles_for_archive_creation: None,
            },
        });

        let mut ckusdc_ledger_builder = PicCanisterBuilder::new("ckusdc_ledger");
        ckusdc_ledger_builder.wasm_path = PicCanister::workspace_dir()
            .join("target/ic/ledger.wasm")
            .to_string_lossy()
            .to_string();
        ckusdc_ledger_builder.arg = encode_one(ckusdc_ledger_arg).unwrap();
        ckusdc_ledger_builder.controllers = Some(vec![controller]);
        ckusdc_ledger_builder.canister_id = Some(ckusdc_ledger_id);
        let ckusdc_ledger_can = ckusdc_ledger_builder.deploy_to(&pic.clone());

        let internal_ledger_id = Principal::from_text(VUSD_LEDGER).unwrap();

        let minter_config = MinterConfig {
            ledger_canister: internal_ledger_id,
            authorized_callers: vec![controller, clearing.canister_id()],
        };

        let minter = PicCanisterBuilder::new("minter")
            .with_arg(encode_one(minter_config).unwrap())
            .with_controllers(vec![controller])
            .deploy_to(&pic.clone());

        // Link clearing to registry
        clearing
            .update::<(), _>(
                controller,
                "set_registry_canister",
                (registry.canister_id(),),
            )
            .expect("Failed to link registry");

        let mut ledgers = BTreeMap::new();
        ledgers.insert(VUSD_ASSET_ID.to_owned(), ledger_can);
        ledgers.insert("ICP".to_owned(), icp_ledger_can);
        ledgers.insert("ckUSDC".to_owned(), ckusdc_ledger_can);

        let setup = Self {
            pic,
            clearing,
            registry,
            minter,
            ledgers,
            user,
            controller,
        };

        // Initialize standard assets
        setup.setup_vusd();
        setup.setup_icp();
        setup.setup_ckusdc();

        setup
    }
}

pub fn test_user(id: u8) -> Principal {
    Principal::from_slice(&[id, 1, 1])
}
