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
