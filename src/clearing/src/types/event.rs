use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::SeriesId;

use crate::types::user::User;

/// Categories of significant events in the clearing system.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum EventType {
    /// A new order was recorded.
    OrderPlaced,
    /// A trade was matched and executed.
    Executed,
    /// A series was settled.
    Settled,
    /// A position was liquidated due to insufficient margin.
    Liquidated,
}

/// A recorded event in the clearing system.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    /// Unique identifier for the event.
    pub event_id: u64,
    /// The principal of the clearing canister.
    pub clearing_id: Principal,
    /// The series ID associated with the event.
    pub series_id: SeriesId,
    /// The user associated with the event.
    pub user: User,
    /// The quantity involved in the event.
    pub qty: i128,
    /// The price at which the event occurred (if applicable).
    pub price: u64,
    /// The type of the event.
    pub event_type: EventType,
    /// Timestamp in nanoseconds since UNIX epoch.
    pub timestamp: u64,
}
