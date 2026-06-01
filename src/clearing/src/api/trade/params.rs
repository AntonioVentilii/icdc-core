use candid::{CandidType, Deserialize, Nat};
use serde::Serialize;
use shared::types::{OutcomeId, Price, SeriesId};

use crate::{
    api::trade::results::TradeHistoryCursor,
    types::{
        trade::{OrderId, Side, TradeId, TransferId},
        user::User,
    },
};

/// Input parameters for submitting a limit order.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitLimitOrderParams {
    /// Unique identifier for the order.
    pub order_id: OrderId,
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// The specific outcome for categorical markets.
    pub outcome_id: Option<OutcomeId>,
    /// The side of the order (Buy or Sell).
    pub side: Side,
    /// The quantity of the order. Must be positive.
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
    /// The specific outcome for categorical markets.
    pub outcome_id: Option<OutcomeId>,
    /// The user opening or increasing a Long position (buyer).
    pub buyer: User,
    /// The user opening or increasing a Short position (seller).
    pub seller: User,
    /// The quantity of the trade. Must be positive.
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
    /// The specific outcome for categorical markets.
    pub outcome_id: Option<OutcomeId>,
    /// Optional valuation price to include in the proof.
    pub valuation_price: Option<Price>,
}

/// Input parameters for cancelling a limit order.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CancelLimitOrderParams {
    /// Unique identifier for the order to cancel.
    pub order_id: OrderId,
}

/// Input parameters for listing active limit orders.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ListOrdersParams {
    /// Optional series identifier to filter orders.
    pub series_id: Option<SeriesId>,
}

/// Input parameters for
/// [`list_series_trade_history`](super::list_series_trade_history).
///
/// Returns the market-wide executed-trade history for a single series so a
/// front end can derive a price-history series (e.g. a YES-probability
/// sparkline) from the per-trade `price`/`timestamp`. Unlike `get_trade_history`
/// the result is not caller-scoped.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ListSeriesTradeHistoryParams {
    /// The derivative series whose executed trades to return.
    pub series_id: SeriesId,
    /// Resume after this cursor (exclusive). `None` starts from the earliest
    /// executed trade. Pass the previous response's `next_cursor` to continue.
    pub start_after: Option<TradeHistoryCursor>,
    /// Maximum number of events to return. `None` returns all remaining events.
    pub limit: Option<u64>,
}
