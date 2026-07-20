use candid::{decode_one, encode_one, Nat, Principal};
use clearing::{
    api::{
        admin::{
            params::{
                RegisterIcrcAssetParams, UpdateAssetMetricsParams, UpdateCollateralAssetParams,
            },
            results::RegisterIcrcAssetResult,
        },
        collateral::{params::DepositCollateralParams, results::DepositCollateralResult},
        trade::{params::SubmitLimitOrderParams, results::SubmitMatchedTradeResult},
    },
    types::{
        trade::{OrderId, Side},
        user::DepositId,
    },
};
use icrc_ledger_types::icrc2::approve::ApproveArgs;
use shared::types::{
    engine::{
        EngineId, EngineResult, EngineRole, GrantEngineRoleParams, RegisterEngineParams,
        RegisterEngineResult,
    },
    evm::NativeEvmAsset,
    series::{AddSeriesParams, AddSeriesResult, ForkSeriesParams},
    Asset, AssetMetrics, BalanceDomain, CollateralAssetConfig, DecimalValue, Description,
    PayoffType, PayoutUnit, Price, Resolution, SeriesId, TradingAccess,
};

use super::{
    constants::VUSD_ASSET_ID, pic_canister::PicCanisterTrait as _, test_environment::TestSetup,
};
use crate::utils::constants::{CKUSDC_LEDGER, ICP_LEDGER};

/// Arguments for [`TradeHelperTrait::setup_icrc_asset`] (keeps call sites readable for clippy).
pub struct SetupIcrcAsset<'a> {
    pub asset_id: &'a str,
    pub ledger_id: Principal,
    pub price_usd_value: u128,
    pub price_decimals: u8,
    pub haircut_bps: u16,
    pub latest_transfer_fee: Option<u128>,
    pub allowed_balance_domains: Vec<BalanceDomain>,
}

pub trait TradeHelperTrait {
    fn setup_vusd(&self);
    fn setup_icp(&self);
    fn setup_ckusdc(&self);
    fn deposit_collateral(
        &self,
        user: Principal,
        asset_id: &str,
        amount: Nat,
        domain: Option<BalanceDomain>,
    );
    fn add_binary_series(
        &self,
        underlying: &str,
        strike_value: u128,
        balance_domain: BalanceDomain,
    ) -> SeriesId;
    /// Registers a `Linear` (forward/NDF/future) series with the given
    /// settlement cap (6-decimal value), returning its id.
    fn add_linear_series(
        &self,
        underlying: &str,
        settlement_cap_value: u128,
        balance_domain: BalanceDomain,
    ) -> SeriesId;
    fn setup_icrc_asset(&self, args: SetupIcrcAsset<'_>);
    fn setup_evm_asset(
        &self,
        asset_id: &str,
        symbol: &str,
        decimals: u8,
        price_usd_value: u128,
        price_decimals: u8,
        haircut_bps: u16,
    );
    fn submit_limit_order(
        &self,
        user: Principal,
        order_id: &str,
        series_id: SeriesId,
        side: Side,
        qty: i128,
        price_value: u128,
    ) -> SubmitMatchedTradeResult;
    fn register_engine(&self, name: &str, allowed_roles: Vec<EngineRole>) -> EngineId;
    fn grant_engine_role(&self, engine_id: &EngineId, principal: Principal, role: EngineRole);
    fn fork_binary_series(
        &self,
        caller: Principal,
        source_id: &SeriesId,
        trading_access: Vec<TradingAccess>,
        engine_id: Option<EngineId>,
    ) -> AddSeriesResult;
}

impl TradeHelperTrait for TestSetup {
    fn setup_vusd(&self) {
        // vUSD is already initialized in Config. We only need to set its price metrics.
        self.clearing
            .update::<(), _>(
                self.controller,
                "update_asset_metrics",
                (UpdateAssetMetricsParams {
                    asset_id: "vUSD".to_owned(),
                    metrics: AssetMetrics {
                        price_usd: DecimalValue::new(1_000_000, 6), // $1.00
                        latest_transfer_fee: Some(0),
                        haircut_bps: 0,
                        insurance_fee_ratio: None,
                        protocol_fee_ratio: None,
                        last_updated_ns: None,
                    },
                },),
            )
            .unwrap();
    }

    fn setup_icp(&self) {
        self.setup_icrc_asset(SetupIcrcAsset {
            asset_id: "ICP",
            ledger_id: Principal::from_text(ICP_LEDGER).unwrap(),
            price_usd_value: 15_000_000,
            price_decimals: 6,
            haircut_bps: 200, // 2% haircut
            latest_transfer_fee: Some(10_000),
            allowed_balance_domains: vec![BalanceDomain::Settlement, BalanceDomain::Playground],
        });
    }

    fn setup_ckusdc(&self) {
        self.setup_icrc_asset(SetupIcrcAsset {
            asset_id: "ckUSDC",
            ledger_id: Principal::from_text(CKUSDC_LEDGER).unwrap(),
            price_usd_value: 1_000_000,
            price_decimals: 6,
            haircut_bps: 100, // 1% haircut
            latest_transfer_fee: Some(10),
            allowed_balance_domains: vec![BalanceDomain::Settlement, BalanceDomain::Playground],
        });
    }

    fn deposit_collateral(
        &self,
        user: Principal,
        asset_id: &str,
        amount: Nat,
        domain: Option<BalanceDomain>,
    ) {
        assert!(
            asset_id != VUSD_ASSET_ID,
            "Manual vUSD deposits are prohibited by policy. Use other collateral assets in tests."
        );

        let ledger_canister = self
            .ledgers
            .get(asset_id)
            .expect("Ledger not found")
            .canister_id();

        // Approve (with buffer for fees)
        let approve_amount = amount.clone() + 100_000_u64;
        let approve_args = ApproveArgs {
            from_subaccount: None,
            spender: self.clearing.canister_id().into(),
            amount: approve_amount,
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        };
        self.pic
            .update_call(
                ledger_canister,
                user,
                "icrc2_approve",
                encode_one(approve_args).unwrap(),
            )
            .unwrap();
        self.pic.tick();

        // Deposit
        let deposit_res: DepositCollateralResult = self
            .clearing
            .update(
                user,
                "deposit_collateral",
                (DepositCollateralParams {
                    deposit_id: DepositId(format!(
                        "dep_{}_{}_{:?}",
                        asset_id,
                        user,
                        self.pic.get_time()
                    )),
                    asset_id: asset_id.to_owned(),
                    amount,
                    domain,
                },),
            )
            .unwrap();

        match deposit_res {
            DepositCollateralResult::Ok => {}
            DepositCollateralResult::Err(err) => panic!("Deposit failed for {asset_id}: {err:?}"),
        }
    }

    fn add_binary_series(
        &self,
        underlying: &str,
        strike_value: u128,
        balance_domain: BalanceDomain,
    ) -> SeriesId {
        let series_params = AddSeriesParams {
            resolution: Resolution::new("Resolved per oracle at expiry"),
            underlying: underlying.to_owned(),
            balance_domain,
            expiry_ns: 2_000_000_000_000_000_000,
            start_ns: None,
            payoff_type: PayoffType::Binary,
            strike: Some(Price::new(strike_value, 6)),
            settlement_cap: None,
            price_precision: 6,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "Chainlink".to_owned(),
            title: format!("{underlying} Binary"),
            description: Description::plain("Test"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            trading_access: vec![],
            engine_id: None,
            locale: None,
        };

        let res_bytes = self
            .pic
            .update_call(
                self.registry.canister_id(),
                self.controller,
                "add_series",
                encode_one(series_params).unwrap(),
            )
            .expect("Registry add_series call failed");

        let add_series_res: AddSeriesResult =
            decode_one(&res_bytes).unwrap_or_else(|_| panic!("Failed to decode add_series result"));
        let series_id = match add_series_res {
            AddSeriesResult::Ok(id) => id,
            AddSeriesResult::Err(e) => panic!("Add series failed: {e:?}"),
        };
        self.pic.tick();
        series_id
    }

    fn add_linear_series(
        &self,
        underlying: &str,
        settlement_cap_value: u128,
        balance_domain: BalanceDomain,
    ) -> SeriesId {
        let series_params = AddSeriesParams {
            resolution: Resolution::new("Settles to oracle spot at expiry"),
            underlying: underlying.to_owned(),
            balance_domain,
            expiry_ns: 2_000_000_000_000_000_000,
            start_ns: None,
            payoff_type: PayoffType::Linear,
            strike: None,
            settlement_cap: Some(Price::new(settlement_cap_value, 6)),
            price_precision: 6,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "Chainlink".to_owned(),
            title: format!("{underlying} Forward"),
            description: Description::plain("Test"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            trading_access: vec![],
            engine_id: None,
            locale: None,
        };

        let res_bytes = self
            .pic
            .update_call(
                self.registry.canister_id(),
                self.controller,
                "add_series",
                encode_one(series_params).unwrap(),
            )
            .expect("Registry add_series call failed");

        let add_series_res: AddSeriesResult =
            decode_one(&res_bytes).unwrap_or_else(|_| panic!("Failed to decode add_series result"));
        let series_id = match add_series_res {
            AddSeriesResult::Ok(id) => id,
            AddSeriesResult::Err(e) => panic!("Add linear series failed: {e:?}"),
        };
        self.pic.tick();
        series_id
    }

    fn setup_icrc_asset(&self, args: SetupIcrcAsset<'_>) {
        let SetupIcrcAsset {
            asset_id,
            ledger_id,
            price_usd_value,
            price_decimals,
            haircut_bps,
            latest_transfer_fee,
            allowed_balance_domains,
        } = args;

        let res: RegisterIcrcAssetResult = self
            .clearing
            .update(
                self.controller,
                "register_icrc_asset",
                (RegisterIcrcAssetParams {
                    asset_id: asset_id.to_owned(),
                    ledger_id,
                    haircut_bps,
                    oracle_id: None,
                    is_enabled: true,
                    allowed_balance_domains,
                },),
            )
            .unwrap();

        match res {
            RegisterIcrcAssetResult::Ok => {}
            RegisterIcrcAssetResult::Err(e) => panic!("ICRC asset registration error: {e:?}"),
        }

        self.pic.tick();

        self.clearing
            .update::<(), _>(
                self.controller,
                "update_asset_metrics",
                (UpdateAssetMetricsParams {
                    asset_id: asset_id.to_owned(),
                    metrics: AssetMetrics {
                        price_usd: DecimalValue::new(price_usd_value, price_decimals),
                        latest_transfer_fee,
                        haircut_bps,
                        insurance_fee_ratio: None,
                        protocol_fee_ratio: None,
                        last_updated_ns: None,
                    },
                },),
            )
            .unwrap();

        self.pic.tick();
    }

    fn setup_evm_asset(
        &self,
        asset_id: &str,
        symbol: &str,
        decimals: u8,
        price_usd_value: u128,
        price_decimals: u8,
        haircut_bps: u16,
    ) {
        let params = UpdateCollateralAssetParams {
            config: CollateralAssetConfig {
                asset_id: asset_id.to_owned(),
                asset: Asset::NativeEvm(NativeEvmAsset {
                    chain_id: 1,
                    decimals,
                }),
                symbol: symbol.to_owned(),
                decimals,
                is_enabled: true,
                oracle_id: None,
                allowed_balance_domains: vec![BalanceDomain::Settlement, BalanceDomain::Playground],
            },
        };

        self.clearing
            .update::<(), _>(self.controller, "update_collateral_asset", (params,))
            .unwrap();

        let metrics_params = UpdateAssetMetricsParams {
            asset_id: asset_id.to_owned(),
            metrics: AssetMetrics {
                price_usd: DecimalValue::new(price_usd_value, price_decimals),
                latest_transfer_fee: None,
                haircut_bps,
                insurance_fee_ratio: None,
                protocol_fee_ratio: None,
                last_updated_ns: None,
            },
        };

        self.clearing
            .update::<(), _>(self.controller, "update_asset_metrics", (metrics_params,))
            .unwrap();

        self.pic.tick();
    }

    fn submit_limit_order(
        &self,
        user: Principal,
        order_id: &str,
        series_id: SeriesId,
        side: Side,
        qty: i128,
        price_value: u128,
    ) -> SubmitMatchedTradeResult {
        self.clearing
            .update(
                user,
                "submit_limit_order",
                (SubmitLimitOrderParams {
                    order_id: OrderId::from(order_id.to_owned()),
                    series_id,
                    outcome_id: None,
                    side,
                    qty,
                    price: Price::new(price_value, 6),
                },),
            )
            .unwrap()
    }

    fn register_engine(&self, name: &str, allowed_roles: Vec<EngineRole>) -> EngineId {
        let res: RegisterEngineResult = self
            .registry
            .update(
                self.controller,
                "register_engine",
                (RegisterEngineParams {
                    name: name.to_owned(),
                    description: None,
                    icon_url: None,
                    admins: vec![self.controller],
                    allowed_roles,
                },),
            )
            .unwrap();

        match res {
            RegisterEngineResult::Ok(id) => id,
            RegisterEngineResult::Err(e) => panic!("register_engine failed: {e:?}"),
        }
    }

    fn grant_engine_role(&self, engine_id: &EngineId, grantee: Principal, role: EngineRole) {
        let res: EngineResult = self
            .registry
            .update(
                self.controller,
                "grant_engine_role",
                (GrantEngineRoleParams {
                    engine_id: engine_id.clone(),
                    grantee,
                    role,
                },),
            )
            .unwrap();

        match res {
            EngineResult::Ok => {}
            EngineResult::Err(e) => {
                panic!("grant_engine_role failed: {e:?}")
            }
        }
    }

    fn fork_binary_series(
        &self,
        caller: Principal,
        source_id: &SeriesId,
        trading_access: Vec<TradingAccess>,
        engine_id: Option<EngineId>,
    ) -> AddSeriesResult {
        let fork_params = ForkSeriesParams {
            resolution: None,
            source_series_id: source_id.clone(),
            title: None,
            description: None,
            trading_access,
            engine_id,
            locale: None,
        };

        let res_bytes = self
            .pic
            .update_call(
                self.registry.canister_id(),
                caller,
                "fork_series",
                encode_one(fork_params).unwrap(),
            )
            .expect("Registry fork_series call failed");

        decode_one(&res_bytes).unwrap_or_else(|_| panic!("Failed to decode fork_series result"))
    }
}
