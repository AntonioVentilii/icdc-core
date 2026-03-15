use crate::payoffs::types::RoundingMode;

/// Scales a value from `source_precision` to the target asset decimals.
pub(crate) fn scale_price(
    value: u128,
    target_decimals: u32,
    source_precision: u32,
    mode: RoundingMode,
) -> u128 {
    if target_decimals >= source_precision {
        value * 10_u128.pow(target_decimals - source_precision)
    } else {
        let factor = 10_u128.pow(source_precision - target_decimals);
        match mode {
            RoundingMode::Floor => value / factor,
            RoundingMode::Ceil => value.div_ceil(factor),
        }
    }
}
