use candid::{decode_one, encode_one, Nat, Principal};
use clearing::{
    api::{
        admin::{
            params::{
                RegisterIcrcAssetParams, UpdateAssetMetricsParams, UpdateAssetPriceParams,
                UpdateCollateralAssetParams,
            },
            results::{RegisterIcrcAssetResult, UpdateAssetPriceResult},
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
use registry::{AddSeriesParams, AddSeriesResult};
use shared::types::{
    evm::NativeEvmAsset, Asset, AssetMetrics, BalanceDomain, CollateralAssetConfig, DecimalValue,
    Description, PayoffType, PayoutUnit, Price, SeriesId,
};

use super::{pic_canister::PicCanisterTrait as _, test_environment::TestSetup};

pub trait TradeHelperTrait {
    fn setup_vusd(&self);
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
    fn setup_icrc_asset(
        &self,
        asset_id: &str,
        ledger_id: Principal,
        price_usd_value: u128,
        price_decimals: u8,
        haircut_bps: u16,
        latest_transfer_fee: Option<u128>,
    );
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
}

impl TradeHelperTrait for TestSetup {
    fn setup_vusd(&self) {
        let vusd_ledger = self
            .ledgers
            .get("vUSD")
            .expect("vUSD ledger not found")
            .canister_id();

        // 1. Register vUSD in Clearing
        let res: RegisterIcrcAssetResult = self
            .clearing
            .update(
                self.controller,
                "register_icrc_asset",
                (RegisterIcrcAssetParams {
                    asset_id: "vUSD".to_owned(),
                    ledger_id: vusd_ledger,
                    haircut_bps: 0,
                    oracle_id: Some("vUSD/USD".to_owned()),
                    is_enabled: true,
                },),
            )
            .unwrap();

        match res {
            RegisterIcrcAssetResult::Ok => {}
            RegisterIcrcAssetResult::Err(err) => panic!("vUSD Registration failed: {err:?}"),
        }

        // 2. Set vUSD Price
        self.clearing
            .update::<UpdateAssetPriceResult, _>(
                self.controller,
                "update_asset_price",
                (UpdateAssetPriceParams {
                    asset_id: "vUSD".to_owned(),
                    price: Price::new(1_000_000, 6), // $1.00
                },),
            )
            .unwrap();
    }

    fn deposit_collateral(
        &self,
        user: Principal,
        asset_id: &str,
        amount: Nat,
        domain: Option<BalanceDomain>,
    ) {
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
            underlying: underlying.to_owned(),
            balance_domain,
            expiry_ns: 2_000_000_000_000_000_000,
            payoff_type: PayoffType::Binary,
            strike: Some(Price::new(strike_value, 6)),
            price_precision: 6,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "Chainlink".to_owned(),
            title: format!("{underlying} Binary"),
            description: Description::plain("Test"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
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

    fn setup_icrc_asset(
        &self,
        asset_id: &str,
        ledger_id: Principal,
        price_usd_value: u128,
        price_decimals: u8,
        haircut_bps: u16,
        latest_transfer_fee: Option<u128>,
    ) {
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
}
