use shared::constants::BPS_BASE;

/// Calculates a settlement-related fee (e.g., insurance or protocol fee) for a given payoff.
///
/// # Arguments
/// * `payoff_usd` - The settlement payoff in USD base units (`USD_DECIMALS`).
/// * `fee_ratio_bps` - The fee ratio in basis points (1 bp = 0.01%).
pub fn calculate_settlement_fee(payoff_usd: u128, fee_ratio_bps: u16) -> u128 {
    (payoff_usd * u128::from(fee_ratio_bps)) / u128::from(BPS_BASE)
}
