use shared::types::{PayoffType, Series};

/// Calculates the actual payout at settlement based on the series type.
///
/// Returns the amount (in the settlement asset's base units) that the long side
/// of the position receives.
///
/// # Arguments
/// * `series` - The derivative series details.
/// * `settlement_price` - The final price from the oracle.
/// * `qty` - The net quantity of the position (positive for Long, negative for Short).
pub fn get_settlement_value(series: &Series, settlement_price: u64, qty: i128) -> u128 {
    let abs_qty = qty.unsigned_abs();
    match series.payoff_type {
        PayoffType::Binary => {
            // Binary Options (Digital): 1.0 (100M) if in-the-money, 0 otherwise.
            // But currently the system uses the settlement price as the "probability" or "index"
            // where 100M is the max payout.
            // If qty > 0 (Long), they get: settlement_price
            // If qty < 0 (Short), they get: (100M - settlement_price)
            let max_payoff: u128 = 100_000_000;
            if qty >= 0 {
                abs_qty * (settlement_price as u128)
            } else {
                abs_qty * (max_payoff.saturating_sub(settlement_price as u128))
            }
        }
        PayoffType::Call => {
            // Vanilla Call: max(S - K, 0)
            let strike = series.strike.unwrap_or(0);
            if qty >= 0 {
                // Long Call
                abs_qty * (settlement_price.saturating_sub(strike) as u128)
            } else {
                // Short Call: The short pays max(S-K, 0).
                // In this settlement model, we calculate what the user *receives*.
                // For a short, this calculation might be negative or handle debt.
                // For now, we return 0 as the "payoff" and the settlement logic handles the rest.
                0
            }
        }
        PayoffType::Put => {
            // Vanilla Put: max(K - S, 0)
            let strike = series.strike.unwrap_or(0);
            if qty >= 0 {
                // Long Put
                abs_qty * (strike.saturating_sub(settlement_price) as u128)
            } else {
                // Short Put
                0
            }
        }
    }
}

/// Calculates the required collateral (margin) to hold a position.
///
/// This determines how much of the settlement asset must be locked in the user's
/// margin account to maintain the position.
///
/// # Arguments
/// * `series` - The derivative series details.
/// * `price` - The current market price or entry price.
/// * `qty` - The net quantity of the position.
pub fn get_required_margin(series: &Series, price: u64, qty: i128) -> u128 {
    let abs_qty = qty.unsigned_abs();
    match series.payoff_type {
        PayoffType::Binary => {
            let max_payoff: u128 = 100_000_000;
            if qty > 0 {
                // Long Binary: Must pay the current price.
                abs_qty * (price as u128)
            } else if qty < 0 {
                // Short Binary: Must collateralise the remaining payout.
                abs_qty * (max_payoff.saturating_sub(price as u128))
            } else {
                0
            }
        }
        PayoffType::Call | PayoffType::Put => {
            // Placeholder: Vanilla margin model.
            // For Long: usually the full premium (price).
            // For Short: depends on risk, but often strike + buffer or similar.
            // For now, we use price as a placeholder for both sides.
            abs_qty * (price as u128)
        }
    }
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{PayoffType, SeriesId, SettlementAsset};

    use super::*;

    fn mock_series(payoff_type: PayoffType, strike: Option<u64>) -> Series {
        Series {
            series_id: SeriesId::from("test".to_string()),
            underlying: "ICP".to_string(),
            expiry_ns: 0,
            payoff_type,
            strike,
            settlement_asset: SettlementAsset::Icp,
            oracle_source: "oracle".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 1700000000,
            title: "Test Series".to_string(),
            description: "A test series for unit testing".to_string(),
        }
    }

    #[test]
    fn test_binary_payoff() {
        let series = mock_series(PayoffType::Binary, None);
        // Long 10 units, price 60M -> 600M
        assert_eq!(get_settlement_value(&series, 60_000_000, 10), 600_000_000);
        // Short 10 units, price 60M -> (100M - 60M) * 10 = 400M
        assert_eq!(get_settlement_value(&series, 60_000_000, -10), 400_000_000);
    }

    #[test]
    fn test_call_payoff() {
        let series = mock_series(PayoffType::Call, Some(100));
        // Settlement 150 -> Payoff (150-100) = 50
        assert_eq!(get_settlement_value(&series, 150, 1), 50);
        // Settlement 80 -> Payoff 0
        assert_eq!(get_settlement_value(&series, 80, 1), 0);
    }

    #[test]
    fn test_put_payoff() {
        let series = mock_series(PayoffType::Put, Some(100));
        // Settlement 80 -> Payoff (100-80) = 20
        assert_eq!(get_settlement_value(&series, 80, 1), 20);
        // Settlement 150 -> Payoff 0
        assert_eq!(get_settlement_value(&series, 150, 1), 0);
    }
}
