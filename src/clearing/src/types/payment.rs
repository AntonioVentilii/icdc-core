use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Mechanism for ensuring idempotency in ledger payments.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum PaymentIdempotency {
    /// Uses the ICRC `created_at_time` field to prevent duplicate transfers.
    IcrcCreatedAtTime(u64),
}
impl PaymentIdempotency {
    /// Converts the idempotency key to an optional timestamp.
    pub fn to_created_at_time(&self) -> Option<u64> {
        match self {
            PaymentIdempotency::IcrcCreatedAtTime(time) => Some(*time),
        }
    }
}
impl From<u64> for PaymentIdempotency {
    fn from(value: u64) -> Self {
        PaymentIdempotency::IcrcCreatedAtTime(value)
    }
}

/// Proof of successful payment on a ledger.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum PaymentReceipt {
    /// The block index in which the transfer was recorded.
    IcrcBlockIndex(candid::Nat),
}
impl From<candid::Nat> for PaymentReceipt {
    fn from(value: candid::Nat) -> Self {
        PaymentReceipt::IcrcBlockIndex(value)
    }
}
