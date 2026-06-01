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
