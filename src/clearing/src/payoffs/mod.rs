pub(crate) mod fees;
pub(crate) mod types;
pub(crate) mod utils;

use shared::{
    constants::USD_DECIMALS,
    types::{PayoffType, Price, Series},
};
pub(crate) use types::RoundingMode;
pub(crate) use utils::scale_price;

/// Calculates the actual payout at settlement based on the series type.
///
/// Returns the amount (in the settlement asset's base units) that the long side
/// of the position receives.
///
/// NOTE: Payouts are rounded DOWN (Floor) when scaling from system precision
/// to asset decimals to ensure system solvency.
///
/// # Arguments
/// * `series` - The derivative series details.
/// * `settlement_price` - The final price from the oracle.
/// * `qty` - The net quantity of the position (number of series units, positive for Long, negative
///   for Short).
pub fn get_settlement_value(series: &Series, settlement_price: &Price, qty: i128) -> u128 {
    let abs_qty = qty.unsigned_abs();

    let asset_decimals = USD_DECIMALS as u32;

    let source_precision = settlement_price.decimals() as u32;
    let price_value = settlement_price.value();

    match series.payoff_type {
        PayoffType::Binary => {
            let max_payoff = 10u128.pow(asset_decimals);

            // We use Floor for payouts to be conservative.
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Floor,
            );

            // If qty > 0 (Long), they get: settlement_price
            // If qty < 0 (Short), they get: (max_payoff - settlement_price)
            if qty >= 0 {
                abs_qty * scaled_price
            } else {
                abs_qty * max_payoff.saturating_sub(scaled_price)
            }
        }

        PayoffType::Call => {
            // Vanilla Call: max(S - K, 0)
            let strike_price_value = series
                .strike
                .as_ref()
                .map(|p| {
                    scale_price(
                        p.value(),
                        source_precision,
                        p.decimals() as u32,
                        RoundingMode::Floor,
                    )
                })
                .unwrap_or(0);

            let raw_payoff = settlement_price.value().saturating_sub(strike_price_value);

            let scaled_payoff = scale_price(
                raw_payoff,
                asset_decimals,
                source_precision,
                RoundingMode::Floor,
            );

            abs_qty * scaled_payoff
        }

        PayoffType::Put => {
            // Vanilla Put: max(K - S, 0)
            let strike_price_value = series
                .strike
                .as_ref()
                .map(|p| {
                    scale_price(
                        p.value(),
                        source_precision,
                        p.decimals() as u32,
                        RoundingMode::Floor,
                    )
                })
                .unwrap_or(0);

            let raw_payoff = strike_price_value.saturating_sub(settlement_price.value());

            let scaled_payoff = scale_price(
                raw_payoff,
                asset_decimals,
                source_precision,
                RoundingMode::Floor,
            );

            abs_qty * scaled_payoff
        }
    }
}

/// Calculates the required collateral (margin) to hold a position.
///
/// This determines how much of the settlement asset must be locked in the user's
/// margin account to maintain the position.
///
/// NOTE: Margin requirements are rounded UP (Ceiling) when scaling from system
/// precision to asset decimals to be conservative.
///
/// # Arguments
/// * `series` - The derivative series details.
/// * `price` - The current market price or entry price.
/// * `qty` - The net quantity of the position (number of series units, positive for Long, negative
///   for Short).
pub fn get_required_margin(series: &Series, price: &Price, qty: i128) -> u128 {
    let abs_qty = qty.unsigned_abs();

    let asset_decimals = USD_DECIMALS as u32;

    let source_precision = price.decimals() as u32;
    let price_value = price.value();

    // We use Ceil for margin to be conservative and ensure enough funds are blocked.
    let scaled_price = scale_price(
        price_value,
        asset_decimals,
        source_precision,
        RoundingMode::Ceil,
    );

    match series.payoff_type {
        PayoffType::Binary => {
            let max_payoff = 10u128.pow(asset_decimals);

            if qty > 0 {
                // Long Binary: Must pay the current price.
                abs_qty * scaled_price
            } else if qty < 0 {
                // Short Binary: Must collateralise the remaining payout.
                abs_qty * (max_payoff.saturating_sub(scaled_price))
            } else {
                0
            }
        }

        PayoffType::Call => {
            // Margin for Calls:
            // - Long: Full premium (current price).
            // - Short: In this version, we require full coverage or premium-based collateral.
            abs_qty * scaled_price
        }

        PayoffType::Put => {
            // Margin for Puts:
            // - Long: Full premium (current price).
            // - Short: Seller must collateralise up to the strike price to ensure solvency.
            if qty < 0 {
                if let Some(ref strike) = series.strike {
                    let scaled_strike = scale_price(
                        strike.value(),
                        asset_decimals,
                        strike.decimals() as u32,
                        RoundingMode::Ceil,
                    );
                    abs_qty * scaled_strike
                } else {
                    0
                }
            } else {
                abs_qty * scaled_price
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{PayoffType, PayoutUnit, SeriesId};

    use super::*;

    fn mock_series(
        payoff_type: PayoffType,
        strike: Option<Price>,
        precision: u8,
        payout_unit: PayoutUnit,
    ) -> Series {
        Series {
            series_id: SeriesId::from("test".to_string()),
            underlying: "ICP".to_string(),
            expiry_ns: 0,
            payoff_type,
            strike,
            price_precision: precision,
            payout_unit,
            oracle_source: "oracle".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 1700000000,
            title: "Test Series".to_string(),
            description: "A test series for unit testing".to_string().into(),
        }
    }

    #[test]
    fn test_binary_payoff_usd() {
        let series = mock_series(PayoffType::Binary, None, 8, PayoutUnit::usd());
        let price = Price::new(60_000_000, 8);
        // Price 0.60 (60M). Max 1.0 (1M because USD_DECIMALS=6).
        assert_eq!(get_settlement_value(&series, &price, 10), 6_000_000);
        assert_eq!(get_settlement_value(&series, &price, -10), 4_000_000);
    }

    #[test]
    fn test_call_payoff() {
        let strike_price = Price::new(100_0000_0000, 8); // $100.00
        let settle_price = Price::new(150_0000_0000, 8); // $150.00

        let series_usd = mock_series(
            PayoffType::Call,
            Some(strike_price.clone()),
            8,
            PayoutUnit::usd(),
        );
        // USD: (150 - 100) = 50.00 -> 50_000_000
        assert_eq!(
            get_settlement_value(&series_usd, &settle_price, 1),
            50_000_000
        );
    }

    #[test]
    fn test_put_payoff() {
        let strike_price = Price::new(100_0000_0000, 8); // $100.00
        let settle_price = Price::new(80_0000_0000, 8); // $80.00

        let series_usd = mock_series(
            PayoffType::Put,
            Some(strike_price.clone()),
            8,
            PayoutUnit::usd(),
        );
        // USD: (100 - 80) = 20.00 -> 20_000_000
        assert_eq!(
            get_settlement_value(&series_usd, &settle_price, 1),
            20_000_000
        );
    }

    #[test]
    fn test_margin_logic() {
        let price = Price::new(60_000_000, 8); // 0.60 (8 decimals)
        let series_usd = mock_series(PayoffType::Binary, None, 8, PayoutUnit::usd());
        // Long Binary (6 decimals): 0.60 -> 600,000
        assert_eq!(get_required_margin(&series_usd, &price, 1), 600_000);
        // Short Binary (6 decimals): (1.0 - 0.6) = 0.40 -> 400,000
        assert_eq!(get_required_margin(&series_usd, &price, -1), 400_000);

        // Call Margin
        let call_series = mock_series(
            PayoffType::Call,
            Some(Price::new(50_000_000, 8)),
            8,
            PayoutUnit::usd(),
        );
        // Long Call: premium (0.60)
        assert_eq!(get_required_margin(&call_series, &price, 10), 6_000_000);
        // Short Call: premium (0.60)
        assert_eq!(get_required_margin(&call_series, &price, -10), 6_000_000);

        // Put Margin
        let strike = Price::new(100_000_000, 8); // $1.00
        let put_series = mock_series(PayoffType::Put, Some(strike), 8, PayoutUnit::usd());
        // Long Put: premium (0.60)
        assert_eq!(get_required_margin(&put_series, &price, 10), 6_000_000);
        // Short Put: strike price ($1.00)
        assert_eq!(get_required_margin(&put_series, &price, -10), 10_000_000);
    }

    #[test]
    fn test_rounding_precision() {
        // USD: 6 decimals
        let series_usd = mock_series(PayoffType::Binary, None, 8, PayoutUnit::usd());

        // Use an unaligned price to verify proper rounding (conversion between 8 and 6 decimals).
        // 35,555,555 / 100 = 355,555.55...
        let unaligned_price = Price::new(35_555_555, 8);

        // Margin (Ceil): (35,555,555 + 99) / 100 = 355,556
        assert_eq!(
            get_required_margin(&series_usd, &unaligned_price, 1),
            355_556
        );

        // Settlement (Floor): 35,555,555 / 100 = 355,555
        assert_eq!(
            get_settlement_value(&series_usd, &unaligned_price, 1),
            355_555
        );
    }

    #[test]
    fn test_custom_precision() {
        // Case: 10 decimals price precision, 6 decimals USD
        let series = mock_series(PayoffType::Binary, None, 10, PayoutUnit::usd());

        // Price: 35.5% -> 3,550,000,000 (10 decimals)
        // Scaled to 6 decimals: 3,550,000,000 / 10,000 = 355,000
        let price = Price::new(3_550_000_000, 10);
        assert_eq!(get_required_margin(&series, &price, 1), 355_000);
    }

    #[test]
    fn test_mismatched_precision() {
        // Strike: $100.00 (6 decimals) -> 100,000,000
        let strike_price = Price::new(100_000_000, 6);
        // Settle: $150.00 (8 decimals) -> 15,000,000,000
        let settle_price = Price::new(15_000_000_000, 8);

        let series = mock_series(PayoffType::Call, Some(strike_price), 6, PayoutUnit::usd());

        // (150.00 - 100.00) = 50.00 -> 50,000,000 in USD (6 decimals)
        assert_eq!(get_settlement_value(&series, &settle_price, 1), 50_000_000);

        // Margin for Short Put with mismatched precision
        // Strike: $100.00 (6 decimals)
        // Asset: USD (6 decimals)
        let put_series = mock_series(
            PayoffType::Put,
            Some(Price::new(100_000_000, 6)),
            6,
            PayoutUnit::usd(),
        );
        let current_price = Price::new(80_000_000, 8); // $0.80

        // Short Put margin should be the strike ($100.00) in asset decimals (6)
        assert_eq!(
            get_required_margin(&put_series, &current_price, -1),
            100_000_000
        );
    }
}
