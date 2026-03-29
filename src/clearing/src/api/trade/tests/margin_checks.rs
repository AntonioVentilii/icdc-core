#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use candid::Principal;
    use shared::types::{
        Asset, AssetId, AssetMetrics, BalanceDomain, CollateralAssetConfig, DecimalValue,
    };

    use crate::types::{margin::AccountState, user::User};

    #[test]
    fn margin_consumption_logic() {
        let user = User(Principal::from_slice(&[1]));
        let asset_id = AssetId::from("USDC");

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
                allowed_balance_domains: vec![BalanceDomain::Settlement],
            },
        );

        let mut metrics = BTreeMap::new();
        metrics.insert(
            asset_id.clone(),
            AssetMetrics {
                price_usd: DecimalValue {
                    value: 1_000_000,
                    decimals: 6,
                }, // $1.00
                haircut_bps: 0,
                latest_transfer_fee: None,
                insurance_fee_ratio: None,
                protocol_fee_ratio: None,
                last_updated_ns: None,
            },
        );

        let mut acc = AccountState::new(user);
        acc.set_balance(BalanceDomain::Settlement, asset_id.clone(), 1_000_000_000); // 1000 USDC ($1000)

        // 1. Initial State
        let eq = acc.calculate_equity_usd(BalanceDomain::Settlement, &configs, &metrics);
        let avail = acc.get_available_margin_usd(BalanceDomain::Settlement, &configs, &metrics);
        assert_eq!(eq, 10_000_000); // $1000.0000 (4 USD decimals)
        assert_eq!(avail, 10_000_000);

        // 2. Reserve margin (simulating a limit order for $600)
        acc.set_reserved_margin_usd(BalanceDomain::Settlement, 6_000_000);

        let eq = acc.calculate_equity_usd(BalanceDomain::Settlement, &configs, &metrics);
        let avail = acc.get_available_margin_usd(BalanceDomain::Settlement, &configs, &metrics);
        assert_eq!(eq, 10_000_000); // EQUITY SHOULD REMAIN $1000
        assert_eq!(avail, 4_000_000); // AVAILABLE SHOULD BE $400

        // 3. Check failure condition (simulating logic in submit_limit_order)
        let next_req = 5_000_000; // Another $500
        let target_res = acc.get_reserved_margin_usd(BalanceDomain::Settlement) + next_req;
        let eq_raw = acc.calculate_raw_equity_i128(BalanceDomain::Settlement, &configs, &metrics);
        assert!(eq_raw < target_res.cast_signed()); // $1000 < $1100 -> TRUE (Fail)
    }
}
