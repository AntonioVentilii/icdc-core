use shared::constants::BPS_BASE;

/// Calculates the insurance fee for a given payoff.
///
/// # Arguments
/// * `payoff_usd` - The settlement payoff in USD units (6 decimals).
/// * `fee_ratio_bps` - The fee ratio in basis points (1 bp = 0.01%).
pub fn calculate_insurance_fee(payoff_usd: u128, fee_ratio_bps: u16) -> u128 {
    (payoff_usd * (fee_ratio_bps as u128)) / (BPS_BASE as u128)
}
