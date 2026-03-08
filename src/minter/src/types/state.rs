use candid::{CandidType, Deserialize, Principal};
use serde::Serialize;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub ledger_canister: Principal,
    pub authorized_callers: Vec<Principal>,
}
