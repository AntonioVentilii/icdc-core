use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::types::decimal::DecimalValue;

/// Encapsulates a price with its numerical value and domain-specific metadata.
/// Decouples the mathematical representation (DecimalValue) from the
/// reporting context (timestamp, oracle Source).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Price {
    /// The numeric component of the price.
    pub decimal: DecimalValue,
    /// Optional timestamp when this price was generated/observed.
    pub timestamp: Option<u64>,
    /// Optional identifier for the oracle source.
    pub oracle_id: Option<String>,
}

impl Price {
    /// Creates a new Price instance with just the numeric part.
    pub fn new(value: u64, decimals: u8) -> Self {
        Self {
            decimal: DecimalValue::new(value, decimals),
            timestamp: None,
            oracle_id: None,
        }
    }

    /// Convenience getter for the numeric value.
    pub fn value(&self) -> u64 {
        self.decimal.value
    }

    /// Convenience getter for the decimals.
    pub fn decimals(&self) -> u8 {
        self.decimal.decimals
    }
}

impl From<(u64, u8)> for Price {
    fn from((value, decimals): (u64, u8)) -> Self {
        Self::new(value, decimals)
    }
}
