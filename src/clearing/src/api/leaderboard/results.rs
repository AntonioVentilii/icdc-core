use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

/// One principal's standing within a leaderboard window.
///
/// The shape is intentionally an **aggregate**: it exposes only window totals
/// (net realized `PnL` plus the settled / win counts a front end needs for an
/// accuracy figure) and the principal's rank — never the per-series or
/// per-settlement breakdown.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LeaderboardEntry {
    /// The ranked principal.
    pub principal: Principal,
    /// 1-based rank in the requested window, by net realized `PnL` descending.
    /// Competition ranking: principals with equal `PnL` share a rank, and the
    /// next distinct `PnL` skips the tied positions (e.g. `1, 2, 2, 4`).
    pub rank: u64,
    /// The principal's rank in the immediately preceding period (last week /
    /// last month), for ↑/↓ movement. `None` when the window has no prior
    /// period (all-time) or the principal did not appear in it.
    pub prior_rank: Option<u64>,
    /// Net realized `PnL` over the window, in internal USD (`vUSD`) base units —
    /// the signed sum of the principal's settlement cashflows.
    pub realized_pnl: i128,
    /// Number of settled positions in the window.
    pub settled_count: u64,
    /// Number of those settlements that were net positive. `win_count /
    /// settled_count` is the window accuracy.
    pub win_count: u64,
}

/// One principal's settled-position accuracy over a requested window.
///
/// Mirrors the aggregate-only contract of [`LeaderboardEntry`]: window totals
/// (settled / win counts plus net realized `PnL`) and never a per-settlement
/// breakdown. `win_count / settled_count` is the window accuracy. Only
/// principals with at least one settlement in the window are returned — a
/// member with no settlement contributes nothing to a cohort's accuracy, and
/// the caller already knows the full set it asked for.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SettlementAccuracyEntry {
    /// The aggregated principal.
    pub principal: Principal,
    /// Number of settled positions in the window.
    pub settled_count: u64,
    /// Number of those settlements that were net positive (`cashflow_usd > 0`).
    pub win_count: u64,
    /// Net realized `PnL` over the window, in internal USD (`vUSD`) base units —
    /// the signed sum of the principal's settlement cashflows.
    pub realized_pnl: i128,
}

/// A page of ranked leaderboard standings.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LeaderboardPage {
    /// Entries for this page, in ascending rank (ties broken by principal for a
    /// stable order).
    pub items: Vec<LeaderboardEntry>,
    /// When `Some`, pass back as `start_after` to fetch the next page. `None`
    /// means the last page has been returned.
    pub next_cursor: Option<u64>,
    /// Total number of ranked principals in this window (after any `members`
    /// filter), so a front end can show "rank X of `total`" without paging to
    /// the end.
    pub total: u64,
}
