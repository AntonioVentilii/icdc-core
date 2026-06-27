use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::types::leaderboard::LeaderboardWindow;

/// Input parameters for [`list_leaderboard`](super::list_leaderboard).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ListLeaderboardParams {
    /// Which calendar window to rank: current week, current month, or all time.
    pub window: LeaderboardWindow,
    /// When set, rank **only** within this set of principals (a league /
    /// affiliation member set) instead of the whole population, and include
    /// every listed member — even those with no settlements in the window —
    /// with a zeroed aggregate, so the result covers the full league. When
    /// `None`, the global standings are returned and only principals that
    /// settled at least one position in the window appear.
    ///
    /// The set is caller-controlled and ranked in full, so it is capped at
    /// 10,000 principals — any realistic league is well under this; a longer
    /// list is truncated to the first 10,000.
    pub members: Option<Vec<Principal>>,
    /// Exclusive pagination cursor: the number of leading entries already
    /// returned (i.e. the previous page's `next_cursor`). `None` starts from
    /// the top-ranked entry. Pagination is a snapshot over the window's ranked
    /// order, which is deterministic for a fixed underlying aggregate.
    pub start_after: Option<u64>,
    /// Maximum number of entries to return. `None` returns all remaining
    /// entries; `0` is clamped to `1` so a paging caller always makes forward
    /// progress, mirroring the other clearing list queries.
    pub limit: Option<u64>,
}

/// Input parameters for
/// [`aggregate_settlement_accuracy`](super::aggregate_settlement_accuracy).
///
/// Aggregates each supplied principal's settled-position win/total counts over
/// an arbitrary half-open time window `[from_ts, to_ts)`. Unlike
/// [`ListLeaderboardParams`] — which ranks over fixed calendar windows backed
/// by the maintained `SETTLEMENT_LEADERBOARD` index — this scans the raw event
/// log so a consumer can score a cohort over a bespoke window (e.g. a league
/// "battle" running an arbitrary 7-day span that does not align to a calendar
/// week). The clearing layer ascribes no meaning to the set or the window; how
/// they are chosen is entirely a consumer concern.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AggregateSettlementAccuracyParams {
    /// The principals to aggregate over (e.g. a league's member set). The set
    /// is caller-controlled and scanned in full, so it is capped at 10,000 —
    /// any realistic cohort is well under this; a longer list is truncated to
    /// the first 10,000. Duplicate principals are de-duplicated and never
    /// double-count.
    pub members: Vec<Principal>,
    /// Inclusive lower bound on a settlement's `timestamp` (ns since the Unix
    /// epoch). `None` starts from the earliest settlement.
    pub from_ts: Option<u64>,
    /// Exclusive upper bound on a settlement's `timestamp` (ns). `None` runs
    /// through the latest settlement. The window is half-open `[from_ts,
    /// to_ts)` so two adjacent windows that share an endpoint never both count
    /// the same settlement.
    pub to_ts: Option<u64>,
    /// Optional series allow-list. `None` counts settlements on every series;
    /// `Some(set)` counts only settlements whose `series_id` is in the set, so
    /// a consumer can scope a cohort's accuracy to a market category (e.g. a
    /// battle scoped to one tag passes that tag's series). An empty `Some(vec)`
    /// therefore matches nothing. The clearing layer ascribes no meaning to the
    /// set; how it is chosen is entirely a consumer concern.
    pub series_ids: Option<Vec<String>>,
}
