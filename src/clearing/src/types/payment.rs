use candid::CandidType;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum PaymentIdempotency {
    IcrcCreatedAtTime(u64), // created_at_time of the transfer that initiated the payment
}
impl PaymentIdempotency {
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

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum PaymentReceipt {
    IcrcBlockIndex(candid::Nat),
}
impl From<candid::Nat> for PaymentReceipt {
    fn from(value: candid::Nat) -> Self {
        PaymentReceipt::IcrcBlockIndex(value)
    }
}
