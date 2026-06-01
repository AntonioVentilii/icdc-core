use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{Price, SeriesId};

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
    pub price: Price,
    /// The type of the event.
    pub event_type: EventType,
    /// Timestamp in nanoseconds since UNIX epoch.
    pub timestamp: u64,
}

/// A single executed trade on a series, as surfaced by the market-wide
/// price-history query.
///
/// Each executed trade emits two [`Event`] rows (one per counterparty) that
/// share the same `event_id`, `price`, and `timestamp`. A price history needs
/// only one point per trade, so this collapses the pair to the trade-level
/// facts a front end plots — `price`/`timestamp` for the sparkline, `qty` for
/// optional volume — without exposing either counterparty's principal in a
/// market-wide read.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SeriesTradePoint {
    /// Id of the trade's [`Event`] rows. Strictly increasing in execution
    /// order, so it doubles as the pagination cursor.
    pub event_id: u64,
    /// Execution price of the trade.
    pub price: Price,
    /// Traded quantity (positive).
    pub qty: i128,
    /// Execution timestamp in nanoseconds since UNIX epoch.
    pub timestamp: u64,
}
