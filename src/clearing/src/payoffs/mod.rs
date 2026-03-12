pub(crate) mod fees;
pub(crate) mod types;
pub(crate) mod utils;

use shared::{
    constants::USD_DECIMALS,
    types::{OutcomeId, PayoffType, Price, Series, SettlementInput},
};
pub(crate) use types::RoundingMode;
pub(crate) use utils::scale_price;

use crate::types::margin::Position;

/// Calculates the actual payout at settlement based on the series type and position.
///
/// Returns the amount (in the settlement asset's base units) that the position
/// receives.
///
/// # Arguments
/// * `series` - The derivative series details.
/// * `position` - The specific position being settled.
/// * `settlement` - The settlement data (Price or OutcomeId).
pub fn get_settlement_value(
    series: &Series,
    position: &Position,
    settlement: &SettlementInput,
) -> u128 {
    let qty = position.net_qty;
    let abs_qty = qty.unsigned_abs();

    let asset_decimals = USD_DECIMALS as u32;

    match series.payoff_type {
        PayoffType::Binary | PayoffType::Call | PayoffType::Put => {
            let settlement_price = match settlement {
                SettlementInput::Price(p) => p,
                _ => return 0, // Invalid input for these types
            };

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

                PayoffType::Categorical => unreachable!(),
            }
        }

        PayoffType::Categorical => {
            let resolved_id = match settlement {
                SettlementInput::Outcome(id) => id,
                _ => return 0,
            };

            // Categorical Claim: pays 1 unit if matched, 0 otherwise.
            let unit_payoff = 10u128.pow(asset_decimals);

            match &position.outcome_id {
                Some(id) if id == resolved_id => {
                    // Position matches the winner
                    if qty > 0 {
                        abs_qty * unit_payoff
                    } else {
                        0
                    }
                }
                _ => {
                    // Position lost or is short (shorts in categorical are handled differently if
                    // they exist) But in this spec, we model them as digital
                    // state claims.
                    if qty < 0 {
                        // Short seller of a losing outcome gets the collateral back (1 - 0 = 1
                        // unit) However, we track positions per outcome, so
                        // one is long the outcome. If one is short an
                        // outcome, they pay out 1 if it wins.
                        0
                    } else {
                        0
                    }
                }
            }
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
/// * `price` - The current market price or entry price (for scalar products).
/// * `qty` - The net quantity of the position.
/// * `outcome_id` - The specific outcome ID (for categorical products).
pub fn get_required_margin(
    series: &Series,
    price: &Price,
    qty: i128,
    _outcome_id: &Option<OutcomeId>,
) -> u128 {
    let abs_qty = qty.unsigned_abs();

    let asset_decimals = USD_DECIMALS as u32;

    match series.payoff_type {
        PayoffType::Binary => {
            let source_precision = price.decimals() as u32;
            let price_value = price.value();
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Ceil,
            );
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
            let source_precision = price.decimals() as u32;
            let price_value = price.value();
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Ceil,
            );
            abs_qty * scaled_price
        }

        PayoffType::Put => {
            let source_precision = price.decimals() as u32;
            let price_value = price.value();
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Ceil,
            );

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

        PayoffType::Categorical => {
            // Categorical Claim: Fully collateralised.
            // Long requires paying the price.
            // Short requires 1 unit (the payoff) minus the price gathered from sale.
            // But since trades are handled separately, we just need the margin required to hold.
            // For a single claim (e.g. Yes on A), the max loss is 1 unit.
            // In a fully collateralised system, long pays price, short pays (1-price).
            let unit_payoff = 10u128.pow(asset_decimals);

            let source_precision = price.decimals() as u32;
            let price_value = price.value();
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Ceil,
            );

            if qty > 0 {
                abs_qty * scaled_price
            } else if qty < 0 {
                abs_qty * unit_payoff.saturating_sub(scaled_price)
            } else {
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{Description, PayoffType, PayoutUnit, SeriesId, SettlementInput};

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
            created_at_ns: 1000000000,
            title: "Test".to_string(),
            description: Description::plain("Test Description"),
            outcomes: None,
        }
    }

    #[test]
    fn test_binary_payoff_usd() {
        let series = mock_series(PayoffType::Binary, None, 8, PayoutUnit::usd());
        let price = Price::new(60_000_000, 8);
        let settlement = SettlementInput::Price(price);

        let pos_long = Position {
            user: Principal::anonymous().into(),
            series_id: series.series_id.clone(),
            outcome_id: None,
            net_qty: 10,
            reserved_margin_usd: 0,
        };
        let pos_short = Position {
            user: Principal::anonymous().into(),
            series_id: series.series_id.clone(),
            outcome_id: None,
            net_qty: -10,
            reserved_margin_usd: 0,
        };

        // Price 0.60 (60M). Max 1.0 (1M because USD_DECIMALS=6).
        assert_eq!(
            get_settlement_value(&series, &pos_long, &settlement),
            6_000_000
        );
        assert_eq!(
            get_settlement_value(&series, &pos_short, &settlement),
            4_000_000
        );
    }

    #[test]
    fn test_call_payoff() {
        let strike_price = Price::new(100_0000_0000, 8); // $100.00
        let settle_price = Price::new(150_0000_0000, 8); // $150.00
        let settlement = SettlementInput::Price(settle_price);

        let series_usd = mock_series(
            PayoffType::Call,
            Some(strike_price.clone()),
            8,
            PayoutUnit::usd(),
        );
        let pos = Position {
            user: Principal::anonymous().into(),
            series_id: series_usd.series_id.clone(),
            outcome_id: None,
            net_qty: 1,
            reserved_margin_usd: 0,
        };
        // USD: (150 - 100) = 50.00 -> 50_000_000
        assert_eq!(
            get_settlement_value(&series_usd, &pos, &settlement),
            50_000_000
        );
    }

    #[test]
    fn test_put_payoff() {
        let strike_price = Price::new(100_0000_0000, 8); // $100.00
        let settle_price = Price::new(80_0000_0000, 8); // $80.00
        let settlement = SettlementInput::Price(settle_price);

        let series_usd = mock_series(
            PayoffType::Put,
            Some(strike_price.clone()),
            8,
            PayoutUnit::usd(),
        );
        let pos = Position {
            user: Principal::anonymous().into(),
            series_id: series_usd.series_id.clone(),
            outcome_id: None,
            net_qty: 1,
            reserved_margin_usd: 0,
        };
        // USD: (100 - 80) = 20.00 -> 20_000_000
        assert_eq!(
            get_settlement_value(&series_usd, &pos, &settlement),
            20_000_000
        );
    }

    #[test]
    fn test_margin_logic() {
        let price = Price::new(60_000_000, 8); // 0.60 (8 decimals)
        let series_usd = mock_series(PayoffType::Binary, None, 8, PayoutUnit::usd());
        // Long Binary (6 decimals): 0.60 -> 600,000
        assert_eq!(get_required_margin(&series_usd, &price, 1, &None), 600_000);
        // Short Binary (6 decimals): (1.0 - 0.6) = 0.40 -> 400,000
        assert_eq!(get_required_margin(&series_usd, &price, -1, &None), 400_000);

        // Call Margin
        let call_series = mock_series(
            PayoffType::Call,
            Some(Price::new(50_000_000, 8)),
            8,
            PayoutUnit::usd(),
        );
        // Long Call: premium (0.60)
        assert_eq!(
            get_required_margin(&call_series, &price, 10, &None),
            6_000_000
        );
        // Short Call: premium (0.60)
        assert_eq!(
            get_required_margin(&call_series, &price, -10, &None),
            6_000_000
        );

        // Put Margin
        let strike = Price::new(100_000_000, 8); // $1.00
        let put_series = mock_series(PayoffType::Put, Some(strike), 8, PayoutUnit::usd());
        // Long Put: premium (0.60)
        assert_eq!(
            get_required_margin(&put_series, &price, 10, &None),
            6_000_000
        );
        // Short Put: strike price ($1.00)
        assert_eq!(
            get_required_margin(&put_series, &price, -10, &None),
            10_000_000
        );
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
            get_required_margin(&series_usd, &unaligned_price, 1, &None),
            355_556
        );

        let settlement = SettlementInput::Price(unaligned_price);
        let pos = Position {
            user: Principal::anonymous().into(),
            series_id: series_usd.series_id.clone(),
            outcome_id: None,
            net_qty: 1,
            reserved_margin_usd: 0,
        };
        // Settlement (Floor): 35,555,555 / 100 = 355,555
        assert_eq!(
            get_settlement_value(&series_usd, &pos, &settlement),
            355_555
        );
    }

    #[test]
    fn test_categorical_payoff() {
        let outcome_a = OutcomeId::from("A".to_string());
        let outcome_b = OutcomeId::from("B".to_string());
        let series = mock_series(PayoffType::Categorical, None, 8, PayoutUnit::usd());

        let pos_a = Position {
            user: Principal::anonymous().into(),
            series_id: series.series_id.clone(),
            outcome_id: Some(outcome_a.clone()),
            net_qty: 10,
            reserved_margin_usd: 0,
        };

        let settle_a = SettlementInput::Outcome(outcome_a);
        let settle_b = SettlementInput::Outcome(outcome_b);

        // 10 units of A pays 10.0 USD if A wins
        assert_eq!(get_settlement_value(&series, &pos_a, &settle_a), 10_000_000);
        // 10 units of A pays 0 if B wins
        assert_eq!(get_settlement_value(&series, &pos_a, &settle_b), 0);
    }

    #[test]
    fn test_categorical_margin() {
        let outcome_a = OutcomeId::from("A".to_string());
        let series = mock_series(PayoffType::Categorical, None, 8, PayoutUnit::usd());
        let price = Price::new(40_000_000, 8); // $0.40

        // Long requires paying the price: 10 * 0.40 = 4.0 USD
        assert_eq!(
            get_required_margin(&series, &price, 10, &Some(outcome_a.clone())),
            4_000_000
        );
        // Short requires 1 - price: 10 * (1.0 - 0.40) = 6.0 USD
        assert_eq!(
            get_required_margin(&series, &price, -10, &Some(outcome_a)),
            6_000_000
        );
    }
}
