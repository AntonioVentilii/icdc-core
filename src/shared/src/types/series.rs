use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::types::{description::Description, payout::PayoutUnit, price::Price};

/// A unique identifier for a derivative series.
/// Encapsulates a hex-encoded string derived from series parameters.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct SeriesId(String);
impl From<String> for SeriesId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl SeriesId {
    /// Returns the inner string representation of the series ID.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A unique identifier for an outcome in a categorical market.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct OutcomeId(String);
impl From<String> for OutcomeId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl OutcomeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Defines the payoff structure for a derivative contract.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PayoffType {
    /// A fixed payoff if the condition is met (all-or-nothing).
    Binary,
    /// Payoff based on the positive difference between underlying price and strike.
    Call,
    /// Payoff based on the positive difference between strike and underlying price.
    Put,
    /// A categorical market with multiple mutually exclusive outcomes.
    Categorical,
}
impl PayoffType {
    /// Returns the unique identifier bytes used for ID generation.
    pub fn as_id_bytes(&self) -> &'static [u8] {
        match self {
            PayoffType::Binary => b"BINARY",
            PayoffType::Call => b"CALL",
            PayoffType::Put => b"PUT",
            PayoffType::Categorical => b"CATEGORICAL",
        }
    }
}

/// Input data for settling a series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SettlementInput {
    /// Final price for scalar markets (Binary, Call, Put).
    Price(Price),
    /// Resolved winner for categorical markets.
    Outcome(OutcomeId),
}

/// Defines a specific derivative series (contract).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    /// Unique identifier computed from series parameters.
    pub series_id: SeriesId,
    /// The underlying asset ticker or identifier (e.g., "ICP/USD").
    pub underlying: String,
    /// Expiry timestamp in nanoseconds since UNIX epoch.
    pub expiry_ns: u64,
    /// The mathematical payoff model used for this series.
    pub payoff_type: PayoffType,
    /// Target price for options, if applicable.
    pub strike: Option<Price>,
    /// The canonical number of decimals used for prices and strikes in this series.
    pub price_precision: u8,
    /// The unit in which the contract payoff is expressed.
    pub payout_unit: PayoutUnit,
    /// The defined outcomes for categorical markets (ordered).
    pub outcomes: Option<Vec<OutcomeId>>,
    /// The identifier of the oracle providing the settlement data.
    pub oracle_source: String,
    /// The principal identifier of the series creator.
    pub creator: Principal,
    /// Timestamp of series creation in nanoseconds since UNIX epoch.
    pub created_at_ns: u64,
    /// A short, descriptive title for the series.
    pub title: String,
    /// A detailed description of the series.
    pub description: Description,
}
impl Series {
    /// Generates a unique [`SeriesId`] based on the contract parameters.
    ///
    /// The ID is computed using a SHA-256 hash of all defining parameters,
    /// ensuring that identical series have the same ID while preventing collisions.
    pub fn generate_id(params: SeriesIdParams) -> SeriesId {
        let SeriesIdParams {
            underlying,
            expiry_ns,
            payoff_type,
            strike,
            price_precision,
            payout_unit,
            outcomes,
            oracle_source,
        } = params;
        let mut hasher = Sha256::new();

        // 🔐 Domain separator (versioned for future upgrades)
        hasher.update(b"DERIV_SERIES_V2");

        // Explicit field separators to avoid ambiguity
        hasher.update(b"|UNDERLYING|");
        hasher.update(underlying.as_bytes());

        hasher.update(b"|EXPIRY|");
        hasher.update(expiry_ns.to_be_bytes());

        hasher.update(b"|PAYOFF|");
        hasher.update(payoff_type.as_id_bytes());

        hasher.update(b"|STRIKE|");
        match strike {
            Some(p) => {
                hasher.update(p.value().to_be_bytes());
                hasher.update([p.decimals()]);
            }
            None => hasher.update(b"NONE"),
        }

        hasher.update(b"|PRECISION|");
        hasher.update([price_precision]);

        hasher.update(b"|PAYOUT_UNIT|");
        hasher.update(payout_unit.as_id_bytes());

        hasher.update(b"|OUTCOMES|");
        match outcomes {
            Some(list) => {
                for outcome in list {
                    hasher.update(outcome.as_str().as_bytes());
                    hasher.update(b",");
                }
            }
            None => hasher.update(b"NONE"),
        }

        hasher.update(b"|ORACLE|");
        hasher.update(oracle_source.as_bytes());

        let series_id = hex::encode(hasher.finalize());

        series_id.into()
    }
}

/// Parameters used to generate a unique [`SeriesId`].
pub struct SeriesIdParams<'a> {
    pub underlying: &'a str,
    pub expiry_ns: u64,
    pub payoff_type: &'a PayoffType,
    pub strike: Option<&'a Price>,
    pub price_precision: u8,
    pub payout_unit: &'a PayoutUnit,
    pub outcomes: Option<&'a [OutcomeId]>,
    pub oracle_source: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_series_id_consistency() {
        let underlying = "ICP";
        let expiry = 1735689600;
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let precision = 8;
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";

        let id1 = Series::generate_id(SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
        });

        let id2 = Series::generate_id(SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
        });

        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generate_series_id_different_expiry() {
        let underlying = "ICP";
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let precision = 8;
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";

        let id1 = Series::generate_id(SeriesIdParams {
            underlying,
            expiry_ns: 100,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
        });

        let id2 = Series::generate_id(SeriesIdParams {
            underlying,
            expiry_ns: 200,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
        });

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_series_id_different_precision() {
        let underlying = "ICP";
        let expiry = 100;
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";

        let id1 = Series::generate_id(SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            price_precision: 8,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
        });

        let id2 = Series::generate_id(SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            price_precision: 10,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
        });

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_series_with_metadata() {
        let series = Series {
            series_id: SeriesId::from("test".to_string()),
            underlying: "ICP".to_string(),
            expiry_ns: 1735689600,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 8)),
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            oracle_source: "coingecko".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 1700000000,
            title: "Long ICP Call".to_string(),
            description: Description::plain("A vanilla call option on ICP"),
        };

        assert_eq!(series.title, "Long ICP Call");
        assert_eq!(series.description.plain, "A vanilla call option on ICP");
        assert_eq!(series.creator, Principal::anonymous());
        assert_eq!(series.price_precision, 8);
    }
}
