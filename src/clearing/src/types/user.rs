use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

#[derive(
    CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct User(pub Principal);
impl User {
    pub fn principal(self) -> Principal {
        self.0
    }
}
impl From<Principal> for User {
    fn from(p: Principal) -> Self {
        Self(p)
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DepositId(pub String);

pub type DepositKey = (User, DepositId);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WithdrawalId(pub String);

pub type WithdrawalKey = (User, WithdrawalId);
