use std::collections::BTreeMap;

use candid::Nat;
use shared::{
    constants::{BPS_BASE, USD_DECIMALS},
    types::{AssetId, AssetMetrics, BalanceDomain, CollateralAssetConfig},
};

use crate::{
    api::account::results::{AccountStateResponse, AssetWorth},
    types::margin::AccountState,
};

pub struct AccountService;

impl AccountService {
    #[must_use]
    pub fn build_account_state_response(
        state: AccountState,
        domain: BalanceDomain,
        configs: &BTreeMap<AssetId, CollateralAssetConfig>,
        metrics: &BTreeMap<AssetId, AssetMetrics>,
    ) -> AccountStateResponse {
        let mut asset_worths = Vec::new();
        let target_decimals = u32::from(USD_DECIMALS);

        if let Some(domain_balances) = state.balances.get(&domain) {
            for (asset_id, balance) in domain_balances {
                let mut value_usd = 0_u128;
                let mut pre_haircut_value_usd = 0_u128;
                let mut haircut_bps = 0_u16;

                if let (Some(config), Some(metric)) = (configs.get(asset_id), metrics.get(asset_id))
                {
                    if config.is_enabled {
                        let price_value = metric.price_usd.value;
                        let price_decimals = u32::from(metric.price_usd.decimals);
                        let asset_decimals = u32::from(config.decimals);
                        haircut_bps = metric.haircut_bps;

                        let haircut_multiplier =
                            u128::from(BPS_BASE).saturating_sub(u128::from(metric.haircut_bps));

                        let numerator_pre = Nat::from(*balance) * Nat::from(price_value);
                        let numerator_post = numerator_pre.clone() * Nat::from(haircut_multiplier);

                        let total_source_decimals = asset_decimals + price_decimals;

                        let (v_post_nat, v_pre_nat) = if total_source_decimals >= target_decimals {
                            let diff = total_source_decimals - target_decimals;
                            let divisor_raw = Nat::from(10_u128.pow(diff));
                            let divisor_post = Nat::from(BPS_BASE) * divisor_raw.clone();

                            (numerator_post / divisor_post, numerator_pre / divisor_raw)
                        } else {
                            let diff = target_decimals - total_source_decimals;
                            let multiplier_raw = Nat::from(10_u128.pow(diff));
                            (
                                (numerator_post * multiplier_raw.clone()) / Nat::from(BPS_BASE),
                                numerator_pre * multiplier_raw,
                            )
                        };

                        value_usd = v_post_nat.0.try_into().unwrap_or(u128::MAX);
                        pre_haircut_value_usd = v_pre_nat.0.try_into().unwrap_or(u128::MAX);
                    }
                }

                asset_worths.push(AssetWorth {
                    asset_id: asset_id.clone(),
                    balance: *balance,
                    value_usd,
                    pre_haircut_value_usd,
                    haircut_bps,
                });
            }
        }

        let total_equity_usd = state.calculate_equity_usd(domain, configs, metrics);
        let available_margin_usd = state.get_available_margin_usd(domain, configs, metrics);

        AccountStateResponse {
            state,
            assets: asset_worths,
            total_equity_usd,
            available_margin_usd,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use candid::Principal;
    use shared::types::{
        asset::Asset, decimal::DecimalValue, AssetId, AssetMetrics, BalanceDomain,
        CollateralAssetConfig,
    };

    fn both_domains() -> Vec<BalanceDomain> {
        vec![BalanceDomain::Settlement, BalanceDomain::Playground]
    }

    use crate::{
        account::service::AccountService,
        types::{margin::AccountState, user::User},
    };

    #[test]
    fn build_account_state_response_basic() {
        let mut state = AccountState::new(User::from(Principal::anonymous()));
        let asset_id = AssetId::from("ICP");
        state.set_balance(BalanceDomain::Settlement, asset_id.clone(), 100_000_000); // 1 ICP (8 decimals)

        let mut configs = BTreeMap::new();
        configs.insert(
            asset_id.clone(),
            CollateralAssetConfig {
                asset_id: asset_id.clone(),
                asset: Asset::Icrc(Principal::anonymous()),
                symbol: "ICP".to_owned(),
                decimals: 8,
                is_enabled: true,
                oracle_id: None,
                allowed_balance_domains: both_domains(),
            },
        );

        let mut metrics = BTreeMap::new();
        metrics.insert(
            asset_id.clone(),
            AssetMetrics {
                price_usd: DecimalValue {
                    value: 10_000_000,
                    decimals: 6,
                }, // $10
                haircut_bps: 1000, // 10% haircut
                latest_transfer_fee: None,
                insurance_fee_ratio: None,
                protocol_fee_ratio: None,
                last_updated_ns: None,
            },
        );

        let response = AccountService::build_account_state_response(
            state,
            BalanceDomain::Settlement,
            &configs,
            &metrics,
        );

        assert_eq!(response.assets.len(), 1);
        let worth = &response.assets[0];
        assert_eq!(worth.asset_id, "ICP");
        assert_eq!(worth.balance, 100_000_000);
        assert_eq!(worth.pre_haircut_value_usd, 100_000); // $10
        assert_eq!(worth.value_usd, 90_000); // $9
        assert_eq!(worth.haircut_bps, 1000);
        assert_eq!(response.total_equity_usd, 90_000);
    }

    #[test]
    fn build_account_state_response_high_decimals() {
        let mut state = AccountState::new(User::from(Principal::anonymous()));
        let asset_id = AssetId::from("ETH");
        state.set_balance(
            BalanceDomain::Settlement,
            asset_id.clone(),
            1_000_000_000_000_000_000,
        ); // 1 ETH (18 decimals)

        let mut configs = BTreeMap::new();
        configs.insert(
            asset_id.clone(),
            CollateralAssetConfig {
                asset_id: asset_id.clone(),
                asset: Asset::Icrc(Principal::anonymous()),
                symbol: "ETH".to_owned(),
                decimals: 18,
                is_enabled: true,
                oracle_id: None,
                allowed_balance_domains: both_domains(),
            },
        );

        let mut metrics = BTreeMap::new();
        metrics.insert(
            asset_id.clone(),
            AssetMetrics {
                price_usd: DecimalValue {
                    value: 3_000_000_000, // $3000
                    decimals: 6,
                },
                haircut_bps: 2000, // 20% haircut
                latest_transfer_fee: None,
                insurance_fee_ratio: None,
                protocol_fee_ratio: None,
                last_updated_ns: None,
            },
        );

        let response = AccountService::build_account_state_response(
            state,
            BalanceDomain::Settlement,
            &configs,
            &metrics,
        );

        let worth = &response.assets[0];
        assert_eq!(worth.pre_haircut_value_usd, 30_000_000); // $3000
        assert_eq!(worth.value_usd, 24_000_000); // $2400 (3000 * 0.8)
    }

    #[test]
    fn build_account_state_response_low_decimals() {
        let mut state = AccountState::new(User::from(Principal::anonymous()));
        let asset_id = AssetId::from("USDC");
        state.set_balance(BalanceDomain::Settlement, asset_id.clone(), 1_000_000); // 1 USDC (6 decimals)

        let mut configs = BTreeMap::new();
        configs.insert(
            asset_id.clone(),
            CollateralAssetConfig {
                asset_id: asset_id.clone(),
                asset: Asset::Icrc(Principal::anonymous()),
                symbol: "USDC".to_owned(),
                decimals: 6,
                is_enabled: true,
                oracle_id: None,
                allowed_balance_domains: both_domains(),
            },
        );

        let mut metrics = BTreeMap::new();
        metrics.insert(
            asset_id.clone(),
            AssetMetrics {
                price_usd: DecimalValue {
                    value: 1_000_000, // $1
                    decimals: 6,
                },
                haircut_bps: 0, // 0% haircut
                latest_transfer_fee: None,
                insurance_fee_ratio: None,
                protocol_fee_ratio: None,
                last_updated_ns: None,
            },
        );

        let response = AccountService::build_account_state_response(
            state,
            BalanceDomain::Settlement,
            &configs,
            &metrics,
        );

        let worth = &response.assets[0];
        assert_eq!(worth.pre_haircut_value_usd, 10_000); // $1
        assert_eq!(worth.value_usd, 10_000); // $1
    }

    #[test]
    fn high_haircut_with_decimal_scaling() {
        let mut state = AccountState::new(User::from(Principal::anonymous()));
        let asset_id = AssetId::from("icp");
        // 1 ICP (8 decimals)
        state.set_balance(BalanceDomain::Settlement, asset_id.clone(), 100_000_000);

        let mut configs = BTreeMap::new();
        configs.insert(
            asset_id.clone(),
            CollateralAssetConfig {
                asset_id: asset_id.clone(),
                asset: Asset::Icrc(Principal::anonymous()),
                symbol: "ICP".to_owned(),
                decimals: 8,
                is_enabled: true,
                oracle_id: None,
                allowed_balance_domains: both_domains(),
            },
        );

        let mut metrics = BTreeMap::new();
        metrics.insert(
            asset_id.clone(),
            AssetMetrics {
                price_usd: DecimalValue {
                    value: 300,
                    decimals: 2,
                }, // $3.00
                haircut_bps: 9_000, // 90% haircut
                latest_transfer_fee: None,
                insurance_fee_ratio: None,
                protocol_fee_ratio: None,
                last_updated_ns: None,
            },
        );

        let response = AccountService::build_account_state_response(
            state,
            BalanceDomain::Settlement,
            &configs,
            &metrics,
        );

        let worth = &response.assets[0];
        // 1 ICP * $3.00 = $3.00 = 30_000 (scaled to 4 USD decimals)
        assert_eq!(worth.pre_haircut_value_usd, 30_000);
        // After 90% haircut (10% value remaining): $3.00 * 0.1 = $0.30 = 3_000
        assert_eq!(worth.value_usd, 3_000);
        assert_eq!(worth.haircut_bps, 9_000);
    }
}
