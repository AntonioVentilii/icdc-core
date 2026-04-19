pub(crate) mod fees;
pub(crate) mod types;
pub(crate) mod utils;

use core::cmp::Ordering;

use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::{
    constants::USD_DECIMALS,
    types::{BalanceDomain, OutcomeId, PayoffType, Price, Series, SettlementInput},
};
pub(crate) use types::RoundingMode;
pub(crate) use utils::scale_price;

use crate::types::margin::Position;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PayoffError {
    /// Provided settlement input does not match the series payoff type (e.g., Price for
    /// Categorical).
    InvalidSettlementType,
    /// Required strike price is missing for the series type.
    MissingStrike,
    /// Outcome ID not provided for a categorical market.
    MissingOutcomeId,
    /// Internal math error (overflow/underflow).
    MathError(String),
}

/// Calculates the absolute payoff of the underlying per unit of quantity.
///
/// Returns the value (in USD base units) for a LONG position of 1 unit.
///
/// # Arguments
/// * `series` - The derivative series details.
/// * `outcome_id` - The specific outcome (for categorical markets).
/// * `settlement` - The settlement data (Price or `OutcomeId`).
pub fn get_unit_payoff(
    series: &Series,
    outcome_id: &Option<OutcomeId>,
    settlement: &SettlementInput,
) -> Result<u128, PayoffError> {
    if series.balance_domain == BalanceDomain::Social {
        return Ok(0);
    }

    let asset_decimals = u32::from(USD_DECIMALS);

    match series.payoff_type {
        PayoffType::Binary | PayoffType::Call | PayoffType::Put => {
            let SettlementInput::Price(settlement_price) = settlement else {
                return Err(PayoffError::InvalidSettlementType);
            };

            let source_precision = u32::from(settlement_price.decimals());
            let price_value = settlement_price.value();

            match series.payoff_type {
                PayoffType::Binary => {
                    // Binary Payoff: Pays 1 unit if resolved price > 0, else 0.
                    // We use Floor for system safety (never payout more than 1 unit due to
                    // rounding).
                    Ok(scale_price(
                        price_value,
                        asset_decimals,
                        source_precision,
                        RoundingMode::Floor,
                    ))
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
                                u32::from(p.decimals()),
                                RoundingMode::Floor,
                            )
                        })
                        .ok_or(PayoffError::MissingStrike)?;

                    let raw_payoff = settlement_price.value().saturating_sub(strike_price_value);

                    Ok(scale_price(
                        raw_payoff,
                        asset_decimals,
                        source_precision,
                        RoundingMode::Floor,
                    ))
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
                                u32::from(p.decimals()),
                                RoundingMode::Floor,
                            )
                        })
                        .ok_or(PayoffError::MissingStrike)?;

                    let raw_payoff = strike_price_value.saturating_sub(settlement_price.value());

                    Ok(scale_price(
                        raw_payoff,
                        asset_decimals,
                        source_precision,
                        RoundingMode::Floor,
                    ))
                }

                PayoffType::Categorical => unreachable!(),
            }
        }

        PayoffType::Categorical => {
            let SettlementInput::Outcome(resolved_id) = settlement else {
                return Err(PayoffError::InvalidSettlementType);
            };

            let unit_payoff = 10_u128.pow(asset_decimals);

            match outcome_id {
                Some(id) if id == resolved_id => Ok(unit_payoff),
                _ => Ok(0),
            }
        }
    }
}

/// Calculates the actual payout at settlement for a given position.
///
/// This function returns the total amount of collateral (in USD base units)
/// to be credited to the user's account at the end of the series.
///
/// For Binary/Categorical products (Full Collateral model):
/// - Long: Payout = Qty * `UnitPayoff` (where `UnitPayoff` is 0 or 1)
/// - Short: Payout = Qty * (1.0 - `UnitPayoff`)
///
/// Total System Payout (Long + Short) always equals 1 unit of collateral.
pub fn get_settlement_value(
    series: &Series,
    position: &Position,
    settlement: &SettlementInput,
) -> Result<u128, PayoffError> {
    let qty = position.net_qty;
    let abs_qty = qty.unsigned_abs();
    let asset_decimals = u32::from(USD_DECIMALS);
    let max_payoff = 10_u128.pow(asset_decimals);

    let unit_payoff = get_unit_payoff(series, &position.outcome_id, settlement)?;

    match series.payoff_type {
        PayoffType::Binary | PayoffType::Categorical => {
            // Full Collateral Payoff Relationship:
            // Long + Short = 1.0 unit.
            // This zero-sum relationship is the bedrock of system solvency.
            if qty >= 0 {
                Ok(abs_qty * unit_payoff)
            } else {
                // Short on losing outcome: receives their 1 unit collateral back (1 - 0 = 1).
                // Short on winning outcome: pays out their 1 unit (1 - 1 = 0).
                Ok(abs_qty * max_payoff.saturating_sub(unit_payoff))
            }
        }
        _ => {
            // Scalar products (Call/Put) usually only have positive unit payoffs for Longs
            // and 0 for Shorts in this model.
            Ok(abs_qty * unit_payoff)
        }
    }
}

/// Calculates the required collateral (margin) to hold a position.
///
/// This represents the maximum potential loss for a position given the trade price.
///
/// For Binary/Categorical products (Full Collateral model):
/// - Long: Margin = Qty * Price
/// - Short: Margin = Qty * (1.0 - Price)
///
/// Total System Margin (Long + Short) always equals 1 unit per contract.
pub fn get_required_margin(
    series: &Series,
    price: &Price,
    qty: i128,
    _outcome_id: &Option<OutcomeId>,
) -> Result<u128, PayoffError> {
    if series.balance_domain == BalanceDomain::Social {
        return Ok(0);
    }

    let abs_qty = qty.unsigned_abs();
    let asset_decimals = u32::from(USD_DECIMALS);

    match series.payoff_type {
        PayoffType::Binary => {
            let source_precision = u32::from(price.decimals());
            let price_value = price.value();
            // We use Ceil for margin to ensure the user always has SLIGHTLY MORE
            // than required, protecting the system against micro-insolvencies
            // from precision loss.
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Ceil,
            );
            let max_payoff = 10_u128.pow(asset_decimals);

            match qty.cmp(&0) {
                Ordering::Greater => Ok(abs_qty * scaled_price),
                Ordering::Less => Ok(abs_qty * (max_payoff.saturating_sub(scaled_price))),
                Ordering::Equal => Ok(0),
            }
        }

        PayoffType::Call => {
            let source_precision = u32::from(price.decimals());
            let price_value = price.value();
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Ceil,
            );
            // Long Call has 0 maintenance if paid upfront, but here we track entry price as
            // requirement.
            Ok(abs_qty * scaled_price)
        }

        PayoffType::Put => {
            let source_precision = u32::from(price.decimals());
            let price_value = price.value();
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Ceil,
            );

            if qty < 0 {
                if let Some(strike) = &series.strike {
                    let scaled_strike = scale_price(
                        strike.value(),
                        asset_decimals,
                        u32::from(strike.decimals()),
                        RoundingMode::Ceil,
                    );
                    Ok(abs_qty * scaled_strike)
                } else {
                    Err(PayoffError::MissingStrike)
                }
            } else {
                Ok(abs_qty * scaled_price)
            }
        }

        PayoffType::Categorical => {
            let unit_payoff = 10_u128.pow(asset_decimals);
            let source_precision = u32::from(price.decimals());
            let price_value = price.value();
            let scaled_price = scale_price(
                price_value,
                asset_decimals,
                source_precision,
                RoundingMode::Ceil,
            );

            match qty.cmp(&0) {
                Ordering::Greater => Ok(abs_qty * scaled_price),
                Ordering::Less => Ok(abs_qty * unit_payoff.saturating_sub(scaled_price)),
                Ordering::Equal => Ok(0),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{
        BalanceDomain, Description, OutcomeId, PayoffType, PayoutUnit, Price, Series, SeriesId,
        SettlementInput,
    };

    use crate::{
        payoffs::{get_required_margin, get_settlement_value, PayoffError},
        types::margin::Position,
    };

    fn mock_series(
        payoff_type: PayoffType,
        strike: Option<Price>,
        precision: u8,
        payout_unit: PayoutUnit,
    ) -> Series {
        Series {
            series_id: SeriesId::from("test".to_owned()),
            underlying: "ICP".to_owned(),
            expiry_ns: 0,
            payoff_type,
            strike,
            price_precision: precision,
            payout_unit,
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_000_000_000,
            title: "Test".to_owned(),
            description: Description::plain("Test Description"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![],
            engine_id: None,
            forked_from: None,
        }
    }

    #[test]
    fn binary_payoff_usd() {
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

        // Price 0.60 (60M, 8 dp). Max 1.0 in USD base units (10^USD_DECIMALS).
        assert_eq!(
            get_settlement_value(&series, &pos_long, &settlement).unwrap(),
            60_000
        );
        assert_eq!(
            get_settlement_value(&series, &pos_short, &settlement).unwrap(),
            40_000
        );
    }

    #[test]
    fn call_payoff() {
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
        // USD: (150 - 100) = 50.00 in 4-decimal units
        assert_eq!(
            get_settlement_value(&series_usd, &pos, &settlement).unwrap(),
            500_000
        );
    }

    #[test]
    fn put_payoff() {
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
        // USD: (100 - 80) = 20.00 in 4-decimal units
        assert_eq!(
            get_settlement_value(&series_usd, &pos, &settlement).unwrap(),
            200_000
        );
    }

    #[test]
    fn margin_logic() {
        let price = Price::new(60_000_000, 8); // 0.60 (8 decimals)
        let series_usd = mock_series(PayoffType::Binary, None, 8, PayoutUnit::usd());
        // Long Binary: 0.60 -> 6_000 (4-decimal USD)
        assert_eq!(
            get_required_margin(&series_usd, &price, 1, &None).unwrap(),
            6_000
        );
        // Short Binary: (1.0 - 0.6) = 0.40 -> 4_000
        assert_eq!(
            get_required_margin(&series_usd, &price, -1, &None).unwrap(),
            4_000
        );

        // Call Margin
        let call_series = mock_series(
            PayoffType::Call,
            Some(Price::new(50_000_000, 8)),
            8,
            PayoutUnit::usd(),
        );
        // Long Call: premium (0.60)
        assert_eq!(
            get_required_margin(&call_series, &price, 10, &None).unwrap(),
            60_000
        );
        // Short Call: premium (0.60)
        assert_eq!(
            get_required_margin(&call_series, &price, -10, &None).unwrap(),
            60_000
        );

        // Put Margin
        let strike = Price::new(100_000_000, 8); // $1.00
        let put_series = mock_series(PayoffType::Put, Some(strike), 8, PayoutUnit::usd());
        // Long Put: premium (0.60)
        assert_eq!(
            get_required_margin(&put_series, &price, 10, &None).unwrap(),
            60_000
        );
        // Short Put: strike price ($1.00)
        assert_eq!(
            get_required_margin(&put_series, &price, -10, &None).unwrap(),
            100_000
        );
    }

    #[test]
    fn rounding_precision() {
        // USD: canonical `USD_DECIMALS`
        let series_usd = mock_series(PayoffType::Binary, None, 8, PayoutUnit::usd());

        // Use an unaligned price to verify proper rounding (conversion between 8 and 4 decimals).
        let unaligned_price = Price::new(35_555_555, 8);

        // Margin (Ceil): div_ceil(35_555_555, 10_000)
        assert_eq!(
            get_required_margin(&series_usd, &unaligned_price, 1, &None).unwrap(),
            3556
        );

        let settlement = SettlementInput::Price(unaligned_price);
        let pos = Position {
            user: Principal::anonymous().into(),
            series_id: series_usd.series_id.clone(),
            outcome_id: None,
            net_qty: 1,
            reserved_margin_usd: 0,
        };
        // Settlement (Floor): 35_555_555 / 10_000
        assert_eq!(
            get_settlement_value(&series_usd, &pos, &settlement).unwrap(),
            3555
        );
    }

    #[test]
    fn categorical_payoff() {
        let outcome_a = OutcomeId::from("A".to_owned());
        let outcome_b = OutcomeId::from("B".to_owned());
        let series = mock_series(PayoffType::Categorical, None, 8, PayoutUnit::usd());

        let pos_a = Position {
            user: Principal::anonymous().into(),
            series_id: series.series_id.clone(),
            outcome_id: Some(outcome_a.clone()),
            net_qty: 10,
            reserved_margin_usd: 0,
        };

        let settle_a = SettlementInput::Outcome(outcome_a.clone());
        let settle_b = SettlementInput::Outcome(outcome_b);

        // 10 units of A pays 10.0 USD if A wins
        assert_eq!(
            get_settlement_value(&series, &pos_a, &settle_a).unwrap(),
            100_000
        );
        // 10 units of A pays 0 if B wins
        assert_eq!(get_settlement_value(&series, &pos_a, &settle_b).unwrap(), 0);

        let pos_short_a = Position {
            user: Principal::anonymous().into(),
            series_id: series.series_id.clone(),
            outcome_id: Some(outcome_a),
            net_qty: -10,
            reserved_margin_usd: 0,
        };
        // Short A pays 0 if A wins
        assert_eq!(
            get_settlement_value(&series, &pos_short_a, &settle_a).unwrap(),
            0
        );
        // Short A pays 10.0 if B wins (collateral return)
        assert_eq!(
            get_settlement_value(&series, &pos_short_a, &settle_b).unwrap(),
            100_000
        );
    }

    #[test]
    fn categorical_margin() {
        let outcome_a = OutcomeId::from("A".to_owned());
        let series = mock_series(PayoffType::Categorical, None, 8, PayoutUnit::usd());
        let price = Price::new(40_000_000, 8); // $0.40

        // Long requires paying the price: 10 * 0.40 = 4.0 USD
        assert_eq!(
            get_required_margin(&series, &price, 10, &Some(outcome_a.clone())).unwrap(),
            40_000
        );
        // Short requires 1 - price: 10 * (1.0 - 0.40) = 6.0 USD
        assert_eq!(
            get_required_margin(&series, &price, -10, &Some(outcome_a)).unwrap(),
            60_000
        );
    }

    #[test]
    fn missing_strike() {
        let series = mock_series(PayoffType::Call, None, 8, PayoutUnit::usd());
        let settlement = SettlementInput::Price(Price::new(100, 8));
        let pos = Position {
            user: Principal::anonymous().into(),
            series_id: series.series_id.clone(),
            outcome_id: None,
            net_qty: 1,
            reserved_margin_usd: 0,
        };

        let result = get_settlement_value(&series, &pos, &settlement);
        assert_eq!(result.unwrap_err(), PayoffError::MissingStrike);
    }

    #[test]
    fn invalid_settlement_type() {
        let series = mock_series(PayoffType::Binary, None, 8, PayoutUnit::usd());
        let settlement = SettlementInput::Outcome(OutcomeId::from("A".to_owned()));
        let pos = Position {
            user: Principal::anonymous().into(),
            series_id: series.series_id.clone(),
            outcome_id: None,
            net_qty: 1,
            reserved_margin_usd: 0,
        };

        let result = get_settlement_value(&series, &pos, &settlement);
        assert_eq!(result.unwrap_err(), PayoffError::InvalidSettlementType);
    }
}
