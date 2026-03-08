use candid::{CandidType, Deserialize};
use serde::Serialize;

/// A generic representation of a decimal value with fixed precision.
/// This decouples numeric logic from domain-specific types like Price or Quantity.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DecimalValue {
    /// The numeric value (mantissa).
    pub value: u128,
    /// The number of decimal places (exponent).
    pub decimals: u8,
}

impl DecimalValue {
    /// Creates a new DecimalValue.
    pub fn new(value: u128, decimals: u8) -> Self {
        Self { value, decimals }
    }

    /// Converts the decimal value to a f64.
    pub fn to_f64(&self) -> f64 {
        self.value as f64 / 10f64.powi(self.decimals as i32)
    }
}

impl From<(u128, u8)> for DecimalValue {
    fn from((value, decimals): (u128, u8)) -> Self {
        Self::new(value, decimals)
    }
}
