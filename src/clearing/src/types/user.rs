use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

/// Represents a user of the clearing system, uniquely identified by their [`Principal`].
#[derive(
    CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct User(pub Principal);
impl User {
    /// Returns the underlying [`Principal`].
    pub fn principal(self) -> Principal {
        self.0
    }
}
impl From<Principal> for User {
    fn from(p: Principal) -> Self {
        Self(p)
    }
}

/// Unique identifier for a collateral deposit.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DepositId(pub String);

/// Composite key for looking up deposits.
pub type DepositKey = (User, DepositId);

/// Unique identifier for a collateral withdrawal.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WithdrawalId(pub String);

/// Composite key for looking up withdrawals.
pub type WithdrawalKey = (User, WithdrawalId);
