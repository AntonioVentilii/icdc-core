use std::collections::BTreeMap;

use candid::{CandidType, Principal};
use serde::Deserialize;
use shared::types::{Series, SeriesId};

use crate::types::{
    event::Event,
    margin::{MarginAccount, Position},
    plans::{DepositPlan, SettlementPlan, WithdrawalPlan},
    trade::{TradeId, TransferId},
    user::{DepositKey, User, WithdrawalKey},
};

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct PositionProof {
    pub transfer_id: TransferId,
    pub user: User,
    pub series_id: SeriesId,
    pub qty: i128,
    pub clearing_id: Principal,
    pub signature: Vec<u8>,
}

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct StableState {
    pub positions: Vec<Position>,
    pub accounts: BTreeMap<User, MarginAccount>,
    pub series: BTreeMap<SeriesId, Series>,
    pub events: Vec<Event>,
    pub next_id: u64,
    pub registry: Principal,
    pub deposit_plans: BTreeMap<DepositKey, DepositPlan>,
    pub withdrawal_plans: BTreeMap<WithdrawalKey, WithdrawalPlan>,
    pub executed_trades: BTreeMap<TradeId, u64>,
    pub frozen_transfers: BTreeMap<TransferId, PositionProof>,
    pub accepted_transfers: BTreeMap<TransferId, bool>,
    pub settlement_plans: BTreeMap<SeriesId, SettlementPlan>,
}
