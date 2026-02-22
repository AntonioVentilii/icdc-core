use candid::{CandidType, Principal};
use serde::Deserialize;

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct PositionProof {
    pub user: Principal,
    pub series_id: String,
    pub qty: i128,
    pub clearing_id: Principal,
    pub signature: Vec<u8>,
}
