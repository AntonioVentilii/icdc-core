use candid::{CandidType, Deserialize, Nat};
use serde::Serialize;
use shared::types::{Price, SeriesId};

use crate::types::{
    trade::{OrderId, Side, TradeId, TransferId},
    user::User,
};

/// Input parameters for submitting a limit order.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitLimitOrderParams {
    /// Unique identifier for the order.
    pub order_id: OrderId,
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// The side of the order (Buy or Sell).
    pub side: Side,
    /// The quantity of the order.
    pub qty: i128,
    /// The limit price of the order.
    pub price: Price,
}

/// Input parameters for submitting a market order (taking an existing limit order).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitMarketOrderParams {
    /// Unique identifier for the trade resulting from this match.
    pub trade_id: TradeId,
    /// The identifier of the limit order to be matched.
    pub matching_order_id: OrderId,
}

/// Input parameters for submitting a matched trade from an exchange.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitMatchedTradeParams {
    /// Unique identifier for the trade provided by the exchange.
    pub trade_id: TradeId,
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// The user opening or increasing a Long position (buyer).
    pub buyer: User,
    /// The user opening or increasing a Short position (seller).
    pub seller: User,
    /// The quantity of the trade.
    pub qty: i128,
    /// The execution price of the trade.
    pub price: Price,
    /// Optional amount to atomically unblock for the buyer.
    pub buyer_unblock_amount: Option<Nat>,
    /// Optional amount to atomically unblock for the seller.
    pub seller_unblock_amount: Option<Nat>,
}

/// Input parameters for freezing a position to prepare for a cross-canister transfer.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FreezePositionForTransferParams {
    /// Unique identifier for the transfer operation.
    pub transfer_id: TransferId,
    /// The user whose position is being frozen.
    pub user: User,
    /// The derivative series identifier.
    pub series_id: SeriesId,
}

/// Input parameters for cancelling a limit order.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CancelLimitOrderParams {
    /// Unique identifier for the order to cancel.
    pub order_id: OrderId,
}
