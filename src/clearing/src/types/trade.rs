use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{Price, SeriesId};

use crate::types::user::User;

/// A unique identifier for a matched trade.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct TradeId(String);
impl From<String> for TradeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A unique identifier for a limit order.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct OrderId(String);
impl From<String> for OrderId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A unique identifier for a position transfer operation.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct TransferId(String);
impl From<String> for TransferId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Represents the side of an order or trade.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

/// Represents a limit order stored in the clearing canister.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct LimitOrder {
    pub order_id: OrderId,
    pub creator: User,
    pub series_id: SeriesId,
    pub side: Side,
    /// Number of Series Units (contracts), where 1 unit represents exposure to 1.0 of the
    /// underlying asset. It is positive for buy orders and negative for sell orders.
    pub qty: i128,
    /// Limit price in the precision defined by the associated series (`series.price_precision`).
    pub price: Price,
    /// Amount blocked in collateral (denominated in USD units, 6 decimals).
    /// TODO: rename to `blocked_margin_usd` ???
    pub block_index: u128,
}
