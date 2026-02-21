use candid::CandidType;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CanisterStatus {
    pub version: String,
    pub cycles_balance: u128,
    pub memory_usage_bytes: u64,
    pub heap_memory_usage_bytes: u64,
}
