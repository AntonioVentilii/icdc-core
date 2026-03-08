use candid::{CandidType, Deserialize, Nat, Principal};
use serde::Serialize;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MintParams {
    pub to: Principal,
    pub amount: Nat,
}
