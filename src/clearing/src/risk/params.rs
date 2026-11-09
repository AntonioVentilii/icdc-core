//! Risk parameters, their defaults and their hard bounds (`docs/risk/margin-model.md`, sections 12
//! and 13).
//!
//! Bounds are constants: no caller, administrator or controller, can set a value outside them
//! through the API.

use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::AssetId;

use crate::{
    api::risk::errors::RiskError,
    risk::fixed::{Fixed, FixedError},
};

/// Inclusive `[min, max]` bounds of a parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min: Fixed,
    pub max: Fixed,
}

impl Bounds {
    /// Rejects `value` outside the bounds, naming the parameter.
    pub fn check(self, parameter: &str, value: Fixed) -> Result<(), RiskError> {
        if value < self.min || value > self.max {
            return Err(RiskError::OutOfBounds {
                parameter: parameter.to_owned(),
                min: self.min.to_decimal().unwrap_or_default(),
                max: self.max.to_decimal().unwrap_or_default(),
            });
        }
        Ok(())
    }
}

pub const LAMBDA_BOUNDS: Bounds = Bounds {
    min: Fixed::from_raw(800_000_000_000),
    max: Fixed::from_raw(995_000_000_000),
};
pub const WEEKEND_WEIGHT_BOUNDS: Bounds = Bounds {
    min: Fixed::ZERO,
    max: Fixed::ONE,
};
pub const FLOOR_RATIO_BOUNDS: Bounds = Bounds {
    min: Fixed::from_raw(500_000_000_000),
    max: Fixed::from_raw(1_500_000_000_000),
};
pub const MULTIPLIER_BOUNDS: Bounds = Bounds {
    min: Fixed::ONE,
    max: Fixed::from_int(3),
};
pub const EVENT_WEIGHT_BOUNDS: Bounds = Bounds {
    min: Fixed::ZERO,
    max: Fixed::from_int(10),
};
pub const MPOR_DAYS_MIN: u32 = 1;
pub const MPOR_DAYS_MAX: u32 = 10;

const HOUR_NS: u64 = 3_600 * 1_000_000_000;
pub const TIMELOCK_DURATION_MIN_NS: u64 = 24 * HOUR_NS;
/// Seven days: a compromised or departing risk administrator can delay the appointment of a
/// replacement by at most this long.
pub const TIMELOCK_DURATION_MAX_NS: u64 = 7 * 24 * HOUR_NS;
pub const DEFAULT_TIMELOCK_DURATION_NS: u64 = 48 * HOUR_NS;

/// Maximum length of an underlying identifier.
pub const MAX_UNDERLYING_LEN: usize = 16;

/// Parameters of one underlying.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UnderlyingRiskParams {
    /// EWMA decay (section 6.1).
    pub lambda: Fixed,
    /// Volatility-time weight of Saturdays and Sundays (section 6.3).
    pub weekend_weight: Fixed,
    /// Volatility floor as a fraction of long-run volatility (section 6.2).
    pub floor_ratio: Fixed,
    /// Margin period of risk, in days.
    pub mpor_days: u32,
    /// Manual risk multiplier `k` (section 6.4).
    pub k: Fixed,
    /// Collateral asset whose oracle price feeds the daily history, if any.
    pub oracle_asset_id: Option<AssetId>,
}

impl Default for UnderlyingRiskParams {
    fn default() -> Self {
        Self {
            lambda: Fixed::from_raw(940_000_000_000),
            weekend_weight: Fixed::ONE,
            floor_ratio: Fixed::from_raw(750_000_000_000),
            mpor_days: 3,
            k: Fixed::ONE,
            oracle_asset_id: None,
        }
    }
}

impl UnderlyingRiskParams {
    /// Checks every parameter against its bounds.
    pub fn validate(&self) -> Result<(), RiskError> {
        LAMBDA_BOUNDS.check("lambda", self.lambda)?;
        WEEKEND_WEIGHT_BOUNDS.check("weekend_weight", self.weekend_weight)?;
        FLOOR_RATIO_BOUNDS.check("floor_ratio", self.floor_ratio)?;
        MULTIPLIER_BOUNDS.check("k", self.k)?;
        check_mpor_days(self.mpor_days)
    }
}

/// Parameters that apply to every underlying.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GlobalRiskParams {
    /// Multiplier applied to every underlying (section 6.4).
    pub global_k: Fixed,
    /// Delay of timelocked changes.
    pub timelock_duration_ns: u64,
}

impl Default for GlobalRiskParams {
    fn default() -> Self {
        Self {
            global_k: Fixed::ONE,
            timelock_duration_ns: DEFAULT_TIMELOCK_DURATION_NS,
        }
    }
}

pub fn check_mpor_days(mpor_days: u32) -> Result<(), RiskError> {
    if !(MPOR_DAYS_MIN..=MPOR_DAYS_MAX).contains(&mpor_days) {
        return Err(RiskError::InvalidParameter {
            parameter: "mpor_days".to_owned(),
            reason: format!("must be between {MPOR_DAYS_MIN} and {MPOR_DAYS_MAX}"),
        });
    }
    Ok(())
}

pub fn check_timelock_duration(duration_ns: u64) -> Result<(), RiskError> {
    if !(TIMELOCK_DURATION_MIN_NS..=TIMELOCK_DURATION_MAX_NS).contains(&duration_ns) {
        return Err(RiskError::InvalidParameter {
            parameter: "timelock_duration_ns".to_owned(),
            reason: format!(
                "must be between {TIMELOCK_DURATION_MIN_NS} and {TIMELOCK_DURATION_MAX_NS} nanoseconds"
            ),
        });
    }
    Ok(())
}

/// Canonical form of an underlying identifier: trimmed, upper case, 1 to 16 characters among `A-Z`,
/// `0-9`, `/`, `-` and `_`.
pub fn canonical_underlying(raw: &str) -> Result<String, RiskError> {
    let canonical = raw.trim().to_ascii_uppercase();
    let valid = !canonical.is_empty()
        && canonical.len() <= MAX_UNDERLYING_LEN
        && canonical.bytes().all(|b| {
            b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b'/' | b'-' | b'_')
        });
    if valid {
        Ok(canonical)
    } else {
        Err(RiskError::InvalidUnderlying(raw.to_owned()))
    }
}

impl From<FixedError> for RiskError {
    fn from(error: FixedError) -> Self {
        Self::ArithmeticError(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_underlying, check_mpor_days, check_timelock_duration, UnderlyingRiskParams,
        MULTIPLIER_BOUNDS, TIMELOCK_DURATION_MAX_NS, TIMELOCK_DURATION_MIN_NS,
    };
    use crate::{api::risk::errors::RiskError, risk::fixed::Fixed};

    #[test]
    fn defaults_are_within_bounds() {
        UnderlyingRiskParams::default().validate().unwrap();
    }

    #[test]
    fn multiplier_bounds_are_inclusive() {
        MULTIPLIER_BOUNDS.check("k", Fixed::ONE).unwrap();
        MULTIPLIER_BOUNDS.check("k", Fixed::from_int(3)).unwrap();
        assert!(matches!(
            MULTIPLIER_BOUNDS.check("k", Fixed::from_raw(999_999_999_999)),
            Err(RiskError::OutOfBounds { .. })
        ));
        assert!(matches!(
            MULTIPLIER_BOUNDS.check("k", Fixed::from_raw(3_000_000_000_001)),
            Err(RiskError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn every_parameter_is_bounded() {
        let mut params = UnderlyingRiskParams {
            lambda: Fixed::from_raw(999_000_000_000),
            ..UnderlyingRiskParams::default()
        };
        assert!(params.validate().is_err());
        params = UnderlyingRiskParams {
            weekend_weight: Fixed::from_raw(1_000_000_000_001),
            ..UnderlyingRiskParams::default()
        };
        assert!(params.validate().is_err());
        params = UnderlyingRiskParams {
            floor_ratio: Fixed::from_raw(400_000_000_000),
            ..UnderlyingRiskParams::default()
        };
        assert!(params.validate().is_err());
        assert!(check_mpor_days(0).is_err());
        assert!(check_mpor_days(11).is_err());
        check_mpor_days(10).unwrap();
        check_timelock_duration(TIMELOCK_DURATION_MIN_NS).unwrap();
        check_timelock_duration(TIMELOCK_DURATION_MAX_NS).unwrap();
        assert!(check_timelock_duration(TIMELOCK_DURATION_MIN_NS - 1).is_err());
        assert!(check_timelock_duration(TIMELOCK_DURATION_MAX_NS + 1).is_err());
    }

    #[test]
    fn underlyings_are_canonicalized() {
        assert_eq!(canonical_underlying(" btc ").unwrap(), "BTC");
        assert_eq!(canonical_underlying("eur/usd").unwrap(), "EUR/USD");
        assert!(canonical_underlying("").is_err());
        assert!(canonical_underlying("BTC USD").is_err());
        assert!(canonical_underlying("ABCDEFGHIJKLMNOPQ").is_err());
    }
}
