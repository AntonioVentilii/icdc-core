use std::collections::BTreeMap;

use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{Series, SeriesId};

use crate::types::{
    event::Event,
    margin::{MarginAccount, Position},
    plan::{DepositPlan, WithdrawalPlan},
    user::{DepositId, User, WithdrawalId},
};

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct PositionProof {
    pub user: User,
    pub series_id: SeriesId,
    pub qty: i128,
    pub clearing_id: Principal,
    pub signature: Vec<u8>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct StableState {
    pub positions: Vec<Position>,
    pub accounts: BTreeMap<User, MarginAccount>,
    pub series: BTreeMap<SeriesId, Series>,
    pub events: Vec<Event>,
    pub next_id: u64,
    pub registry: Principal,
    pub deposit_plans: BTreeMap<DepositId, DepositPlan>,
    pub withdrawal_plans: BTreeMap<WithdrawalId, WithdrawalPlan>,
}
