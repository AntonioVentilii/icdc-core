use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::{
    constants::USD_DECIMALS,
    types::{Price, SeriesId},
};

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

impl SeriesTradePoint {
    /// Traded notional of this single trade — `qty · price` — in USD base units
    /// (the `USD_DECIMALS` scale).
    ///
    /// Scales by the decimal *delta* between the price's precision and
    /// `USD_DECIMALS`, so when a series prices in `USD_DECIMALS` (the common
    /// case) the result is exactly `qty · value` — no intermediate
    /// `×10^USD_DECIMALS` that could clamp before a later divide cancels it.
    /// Saturating, so a pathological trade clamps rather than overflows.
    #[must_use]
    pub fn notional_usd_base(&self) -> i128 {
        let value = i128::try_from(self.price.value()).unwrap_or(i128::MAX);
        let base = self.qty.saturating_mul(value);
        let delta = i32::from(USD_DECIMALS) - i32::from(self.price.decimals());

        if delta >= 0 {
            base.saturating_mul(10_i128.saturating_pow(u32::try_from(delta).unwrap_or(0)))
        } else {
            base / 10_i128.saturating_pow(u32::try_from(-delta).unwrap_or(0))
        }
    }
}

/// Running per-series traded-volume totals, maintained alongside the trade
/// index so a batch volume read stays `O(#series)` instead of scanning each
/// series' tape. A pure projection of the executed rows in `EVENTS` (not
/// persisted) — rebuilt on upgrade and folded incrementally on each trade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeriesVolumeAggregate {
    /// Cumulative traded notional (Σ `qty · price`) in USD base units.
    pub notional_usd: i128,
    /// Number of executed trades folded in.
    pub trade_count: u64,
}

impl SeriesVolumeAggregate {
    /// Folds one trade's notional (USD base units) into the running totals.
    pub fn record(&mut self, notional_usd: i128) {
        self.notional_usd = self.notional_usd.saturating_add(notional_usd);
        self.trade_count = self.trade_count.saturating_add(1);
    }
}
