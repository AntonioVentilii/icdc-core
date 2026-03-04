use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Metadata about the current state of a canister.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CanisterStatus {
    /// Semantic version of the canister code.
    pub version: String,
    /// Current cycle balance of the canister.
    pub cycles_balance: u128,
    /// Total memory usage in bytes (including stable memory).
    pub memory_usage_bytes: u64,
    /// Current heap memory usage in bytes.
    pub heap_memory_usage_bytes: u64,
}
