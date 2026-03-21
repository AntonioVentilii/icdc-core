use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Represents a distinct domain for balances and trading logic.
#[derive(
    CandidType,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
)]
pub enum BalanceDomain {
    /// Non-collateralized trading (e.g., using VICI).
    Playground,
    /// Real collateralized trading (e.g., using ckUSDC, ICP).
    #[default]
    Settlement,
}

/// Configurable policy for a balance domain.
///
/// Controls domain-specific behavior such as fee ratios and feature flags.
/// Both domains default to identical policies so risk enforcement is uniform.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DomainPolicy {
    /// Whether deposits of real collateral assets are accepted in this domain.
    pub deposits_enabled: bool,
    /// Whether withdrawals of real collateral assets are allowed from this domain.
    pub withdrawals_enabled: bool,
    /// Optional override for the insurance fund fee ratio (bps).
    /// When `None`, the global `Config` value is used.
    pub insurance_fund_fee_ratio_override: Option<u16>,
    /// Optional override for the protocol fee ratio (bps).
    /// When `None`, the global `Config` value is used.
    pub protocol_fee_ratio_override: Option<u16>,
    /// Human-readable label for this domain.
    pub label: String,
}

impl Default for DomainPolicy {
    fn default() -> Self {
        Self {
            deposits_enabled: true,
            withdrawals_enabled: true,
            insurance_fund_fee_ratio_override: None,
            protocol_fee_ratio_override: None,
            label: String::new(),
        }
    }
}
