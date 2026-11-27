//! Fixed-point arithmetic for the risk engine.
//!
//! [`Fixed`] is a signed 128-bit integer scaled by `10^12`. Multiplication and division truncate
//! toward zero, every operation is checked, and nothing here uses floating point, so any figure the
//! risk engine publishes can be reproduced exactly from public inputs (see
//! `docs/risk/margin-model.md`, section 7.7).

use core::fmt;

use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::DecimalValue;

/// Number of decimals carried by [`Fixed`].
pub const DECIMALS: u8 = 12;

/// `10^DECIMALS`: the raw value of `1.0`.
pub const SCALE: i128 = 1_000_000_000_000;

/// `ln(2)` at 12 decimals, rounded to the nearest raw unit.
const LN_2_RAW: i128 = 693_147_180_560;

/// Upper bound on the terms of the `atanh` series in [`Fixed::ln`]; the series converges long
/// before.
const LN_SERIES_MAX_TERMS: i128 = 64;

/// Errors raised by fixed-point operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedError {
    /// The result does not fit in 128 bits.
    Overflow,
    /// Division by zero.
    DivisionByZero,
    /// Logarithm of a value that is not strictly positive.
    NonPositiveLog,
    /// Square root or unsigned conversion of a negative value.
    Negative,
}

impl fmt::Display for FixedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Overflow => "fixed-point overflow",
            Self::DivisionByZero => "fixed-point division by zero",
            Self::NonPositiveLog => "logarithm of a non-positive value",
            Self::Negative => "negative value where a non-negative one is required",
        };
        f.write_str(text)
    }
}

/// A signed fixed-point number with 12 decimals.
#[derive(
    CandidType,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
)]
pub struct Fixed(i128);

impl Fixed {
    pub const ONE: Self = Self(SCALE);
    pub const ZERO: Self = Self(0);

    /// Wraps a raw value, already scaled by `10^12`.
    #[must_use]
    pub const fn from_raw(raw: i128) -> Self {
        Self(raw)
    }

    /// The raw value, scaled by `10^12`.
    #[must_use]
    pub const fn raw(self) -> i128 {
        self.0
    }

    /// The integer `n` as a fixed-point number.
    #[must_use]
    pub const fn from_int(n: i64) -> Self {
        Self(n as i128 * SCALE)
    }

    /// `num / den` as a fixed-point number, truncated toward zero.
    pub fn from_ratio(num: i128, den: i128) -> Result<Self, FixedError> {
        if den == 0 {
            return Err(FixedError::DivisionByZero);
        }
        num.checked_mul(SCALE)
            .map(|scaled| Self(scaled / den))
            .ok_or(FixedError::Overflow)
    }

    /// Converts a [`DecimalValue`], truncating digits beyond 12 decimals.
    pub fn from_decimal(value: &DecimalValue) -> Result<Self, FixedError> {
        let mantissa = i128::try_from(value.value).map_err(|_| FixedError::Overflow)?;
        let decimals = u32::from(value.decimals);
        let own = u32::from(DECIMALS);
        if decimals <= own {
            let factor = 10_i128
                .checked_pow(own - decimals)
                .ok_or(FixedError::Overflow)?;
            mantissa
                .checked_mul(factor)
                .map(Self)
                .ok_or(FixedError::Overflow)
        } else {
            let factor = 10_i128
                .checked_pow(decimals - own)
                .ok_or(FixedError::Overflow)?;
            Ok(Self(mantissa / factor))
        }
    }

    /// Converts to a [`DecimalValue`] with 12 decimals. Fails on negative values.
    pub fn to_decimal(self) -> Result<DecimalValue, FixedError> {
        u128::try_from(self.0)
            .map(|value| DecimalValue::new(value, DECIMALS))
            .map_err(|_| FixedError::Negative)
    }

    pub fn checked_add(self, other: Self) -> Result<Self, FixedError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(FixedError::Overflow)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, FixedError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(FixedError::Overflow)
    }

    /// `self x other`, truncated toward zero.
    pub fn checked_mul(self, other: Self) -> Result<Self, FixedError> {
        self.0
            .checked_mul(other.0)
            .map(|product| Self(product / SCALE))
            .ok_or(FixedError::Overflow)
    }

    /// `self / other`, truncated toward zero.
    pub fn checked_div(self, other: Self) -> Result<Self, FixedError> {
        Self::from_ratio(self.0, other.0)
    }

    /// `self / n` for an integer `n`, truncated toward zero.
    pub fn checked_div_int(self, n: i128) -> Result<Self, FixedError> {
        if n == 0 {
            return Err(FixedError::DivisionByZero);
        }
        Ok(Self(self.0 / n))
    }

    /// The floor of the exact square root. Fails on negative values.
    pub fn sqrt(self) -> Result<Self, FixedError> {
        let raw = u128::try_from(self.0).map_err(|_| FixedError::Negative)?;
        let scale = u128::try_from(SCALE).map_err(|_| FixedError::Overflow)?;
        let radicand = raw.checked_mul(scale).ok_or(FixedError::Overflow)?;
        let root = isqrt(radicand);
        i128::try_from(root)
            .map(Self)
            .map_err(|_| FixedError::Overflow)
    }

    /// The natural logarithm, for strictly positive values.
    ///
    /// The argument is reduced to `m` in `[1, 2)` by powers of two, then `ln(m) = 2 x atanh((m - 1)
    /// / (m + 1))` is summed until its terms vanish at 12 decimals. Absolute error stays below
    /// `10^-10` for the price ratios the engine works with.
    pub fn ln(self) -> Result<Self, FixedError> {
        if self.0 <= 0 {
            return Err(FixedError::NonPositiveLog);
        }

        let mut exponent: i128 = 0;
        let mut mantissa = self.0;
        while mantissa >= 2 * SCALE {
            mantissa /= 2;
            exponent += 1;
        }
        while mantissa < SCALE {
            mantissa *= 2;
            exponent -= 1;
        }

        let m = Self(mantissa);
        let y = m
            .checked_sub(Self::ONE)?
            .checked_div(m.checked_add(Self::ONE)?)?;
        let y2 = y.checked_mul(y)?;

        let mut sum = y;
        let mut power = y;
        let mut denominator: i128 = 1;
        while denominator < 2 * LN_SERIES_MAX_TERMS {
            power = power.checked_mul(y2)?;
            if power.0 == 0 {
                break;
            }
            denominator += 2;
            sum = sum.checked_add(power.checked_div_int(denominator)?)?;
        }

        let ln_m = Self(sum.0.checked_mul(2).ok_or(FixedError::Overflow)?);
        let ln_power = exponent
            .checked_mul(LN_2_RAW)
            .map(Self)
            .ok_or(FixedError::Overflow)?;
        ln_m.checked_add(ln_power)
    }
}

impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        let scale = SCALE.unsigned_abs();
        write!(f, "{sign}{}.{:012}", magnitude / scale, magnitude % scale)
    }
}

/// Floor of the square root of `n`, by Newton iteration on integers.
fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = x.midpoint(n / x);
    }
    x
}

#[cfg(test)]
mod tests {
    use shared::types::DecimalValue;

    use super::{Fixed, FixedError, SCALE};

    fn approx(actual: Fixed, expected_raw: i128, tolerance_raw: i128) {
        let diff = (actual.raw() - expected_raw).abs();
        assert!(
            diff <= tolerance_raw,
            "expected {expected_raw}, got {}, diff {diff}",
            actual.raw()
        );
    }

    #[test]
    fn decimal_round_trip_and_truncation() {
        let price = Fixed::from_decimal(&DecimalValue::new(10_500_000, 2)).unwrap();
        assert_eq!(price, Fixed::from_int(105_000));
        // Digits beyond 12 decimals are truncated.
        let tiny = Fixed::from_decimal(&DecimalValue::new(1_999, 15)).unwrap();
        assert_eq!(tiny.raw(), 1);
        assert_eq!(
            Fixed::from_raw(1_500_000_000_000).to_decimal().unwrap(),
            DecimalValue::new(1_500_000_000_000, 12)
        );
        assert_eq!(Fixed::from_int(-1).to_decimal(), Err(FixedError::Negative));
    }

    #[test]
    fn mul_and_div_truncate_toward_zero() {
        let third = Fixed::ONE.checked_div(Fixed::from_int(3)).unwrap();
        assert_eq!(third.raw(), 333_333_333_333);
        let negative_third = Fixed::from_int(-1).checked_div(Fixed::from_int(3)).unwrap();
        assert_eq!(negative_third.raw(), -333_333_333_333);
        let product = Fixed::from_raw(1_500_000_000_000)
            .checked_mul(Fixed::from_raw(2_500_000_000_000))
            .unwrap();
        assert_eq!(product, Fixed::from_raw(3_750_000_000_000));
        assert_eq!(
            Fixed::ONE.checked_div(Fixed::ZERO),
            Err(FixedError::DivisionByZero)
        );
        assert_eq!(
            Fixed::from_raw(i128::MAX).checked_mul(Fixed::from_int(2)),
            Err(FixedError::Overflow)
        );
    }

    #[test]
    fn sqrt_is_floor_of_exact_root() {
        assert_eq!(Fixed::from_int(4).sqrt().unwrap(), Fixed::from_int(2));
        assert_eq!(Fixed::ZERO.sqrt().unwrap(), Fixed::ZERO);
        // sqrt(2) = 1.414213562373095...
        assert_eq!(Fixed::from_int(2).sqrt().unwrap().raw(), 1_414_213_562_373);
        // sqrt(0.0001) = 0.01
        assert_eq!(
            Fixed::from_raw(SCALE / 10_000).sqrt().unwrap(),
            Fixed::from_raw(SCALE / 100)
        );
        assert_eq!(Fixed::from_int(-1).sqrt(), Err(FixedError::Negative));
    }

    #[test]
    fn ln_matches_known_values() {
        approx(Fixed::from_int(2).ln().unwrap(), 693_147_180_560, 2);
        // ln(10) = 2.302585092994046
        approx(Fixed::from_int(10).ln().unwrap(), 2_302_585_092_994, 10);
        // ln(0.5) = -0.693147180559945
        approx(
            Fixed::from_raw(SCALE / 2).ln().unwrap(),
            -693_147_180_560,
            2,
        );
        // ln(1.01) = 0.009950330853168
        approx(
            Fixed::from_raw(1_010_000_000_000).ln().unwrap(),
            9_950_330_853,
            10,
        );
        // ln(0.99) = -0.010050335853501
        approx(
            Fixed::from_raw(990_000_000_000).ln().unwrap(),
            -10_050_335_854,
            10,
        );
        assert_eq!(Fixed::ONE.ln().unwrap(), Fixed::ZERO);
        assert_eq!(Fixed::ZERO.ln(), Err(FixedError::NonPositiveLog));
    }

    #[test]
    fn display_prints_twelve_decimals() {
        assert_eq!(
            Fixed::from_raw(1_500_000_000_000).to_string(),
            "1.500000000000"
        );
        assert_eq!(
            Fixed::from_raw(-250_000_000_000).to_string(),
            "-0.250000000000"
        );
    }
}
