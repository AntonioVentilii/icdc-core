use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::SeriesId;

use crate::types::user::User;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum EventType {
    OrderPlaced,
    Executed,
    Settled,
    Liquidated,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub event_id: u64,
    pub clearing_id: Principal,
    pub series_id: SeriesId,
    pub user: User,
    pub qty: i128,
    pub price: u64,
    pub event_type: EventType,
    pub timestamp: u64,
}
