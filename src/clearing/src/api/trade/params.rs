use candid::{CandidType, Deserialize, Nat};
use serde::Serialize;
use shared::types::{OutcomeId, Price, SeriesId};

use crate::types::{
    price_history::PriceHistoryInterval,
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
    /// Resume after this trade's `event_id` (exclusive). `None` starts from the
    /// earliest executed trade. Pass the previous response's `next_cursor` to
    /// continue. Executed-trade `event_id`s are strictly increasing in
    /// execution order, so the bare id is a stable cursor.
    pub start_after: Option<u64>,
    /// Maximum number of trades to return. `None` returns all remaining trades.
    pub limit: Option<u64>,
}

/// Input parameters for
/// [`get_series_price_history`](super::get_series_price_history).
///
/// Returns a series' executed trades aggregated into fixed-width
/// [`PriceHistoryInterval`] time buckets (OHLC + volume + trade count), so a
/// front end can render a time-scoped consensus/price chart directly instead of
/// fetching and re-bucketing the raw per-trade tape. The optional time bounds
/// let the caller request just the window it draws (e.g. the last day) and the
/// `interval` picks the resolution (hourly for short windows, daily for long).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetSeriesPriceHistoryParams {
    /// The derivative series whose executed trades to aggregate.
    pub series_id: SeriesId,
    /// Bucket width: one candle per hour or per day.
    pub interval: PriceHistoryInterval,
    /// Inclusive lower bound on a trade's timestamp (ns). `None` starts from the
    /// series' earliest trade.
    pub start_time: Option<u64>,
    /// Exclusive upper bound on a trade's timestamp (ns). `None` runs through
    /// the series' latest trade.
    pub end_time: Option<u64>,
}
