use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::Price;

use crate::{api::trade::errors::TradeError, types::event::SeriesTradePoint};

/// Outcome of a matched trade submission.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SubmitMatchedTradeResult {
    /// Returns `true` if the trade was successfully processed.
    Ok(bool),
    /// Failed to submit or match the trade.
    Err(TradeError),
}
impl From<Result<bool, TradeError>> for SubmitMatchedTradeResult {
    fn from(value: Result<bool, TradeError>) -> Self {
        match value {
            Ok(v) => SubmitMatchedTradeResult::Ok(v),
            Err(e) => SubmitMatchedTradeResult::Err(e),
        }
    }
}

/// Outcome of a position transfer acceptance.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AcceptPositionTransferResult {
    /// Returns `true` if the transfer was successfully accepted and processed.
    Ok(bool),
    /// Failed to accept the position transfer.
    Err(TradeError),
}
impl From<Result<bool, TradeError>> for AcceptPositionTransferResult {
    fn from(value: Result<bool, TradeError>) -> Self {
        match value {
            Ok(v) => AcceptPositionTransferResult::Ok(v),
            Err(e) => AcceptPositionTransferResult::Err(e),
        }
    }
}

/// A page of executed trades scoped to a single series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct SeriesTradeHistoryPage {
    /// Executed trades for the series in this page, one point per trade, ordered
    /// by `event_id` ascending (i.e. execution order).
    pub items: Vec<SeriesTradePoint>,
    /// When `Some`, pass back as `start_after` to fetch the next page (it is the
    /// `event_id` of the last trade returned). `None` means the last page has
    /// been returned.
    pub next_cursor: Option<u64>,
}

/// One aggregated price-history candle: every executed trade on a series whose
/// timestamp falls in a single fixed-width bucket, summarized into the
/// open/high/low/close + volume a chart plots.
///
/// `close` is the bucket's consensus the front end maps to a 0..1 YES
/// probability (the last trade price in the bucket); `open`/`high`/`low` let it
/// draw candles. Prices on one series share the series' precision, so `high` and
/// `low` are the per-bucket max and min by numeric value.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SeriesPriceCandle {
    /// Start of the bucket (ns since the Unix epoch); see
    /// [`PriceHistoryInterval::bucket_start`]. Candles are returned ascending by
    /// this field.
    pub bucket_start_ns: u64,
    /// Price of the first trade in the bucket (earliest timestamp, ties broken
    /// by execution order).
    pub open: Price,
    /// Highest trade price in the bucket.
    pub high: Price,
    /// Lowest trade price in the bucket.
    pub low: Price,
    /// Price of the last trade in the bucket (latest timestamp, ties broken by
    /// execution order) — the consensus a sparkline/chart plots.
    pub close: Price,
    /// Total traded quantity in the bucket (sum of each trade's positive `qty`).
    pub volume: i128,
    /// Number of executed trades in the bucket.
    pub trade_count: u64,
}

/// A series' executed-trade history aggregated into fixed-width time buckets.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct SeriesPriceHistory {
    /// Buckets that contain at least one trade, ascending by `bucket_start_ns`.
    /// Empty buckets are omitted (the series simply had no trades then), so a
    /// young or untraded market returns an empty vector rather than fabricated
    /// points.
    pub candles: Vec<SeriesPriceCandle>,
}
