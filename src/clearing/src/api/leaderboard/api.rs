use core::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use candid::Principal;
use ic_cdk_macros::query;

use super::{
    params::{AggregateSettlementAccuracyParams, ListLeaderboardParams},
    results::{LeaderboardEntry, LeaderboardPage, SettlementAccuracyEntry},
};
use crate::{
    guards::caller_is_not_anonymous,
    memory::{EVENTS, SETTLEMENT_LEADERBOARD},
    types::{
        event::{Event, EventType},
        leaderboard::{LeaderboardWindow, PnlAggregate},
        user::User,
    },
    utils::system::now_ns,
};

/// The maintained leaderboard index shape: realized `PnL` aggregated per principal
/// within each `(window, period)` bucket.
type LeaderboardIndex = BTreeMap<(LeaderboardWindow, u64), BTreeMap<User, PnlAggregate>>;

/// Upper bound on a caller-supplied `members` (league) filter. The set is
/// caller-controlled and must be materialized and ranked in full, so it is
/// capped to keep a single query within the replica's instruction budget and
/// argument-size limit. A league of any realistic size is far below this;
/// anything longer is truncated to the first `MAX_LEADERBOARD_MEMBERS`.
const MAX_LEADERBOARD_MEMBERS: usize = 10_000;

/// Returns ranked standings for a calendar window (this week / this month / all
/// time), derived from settled-position `PnL`, with each entry's prior-window
/// rank for ↑/↓ deltas and stable cursor pagination.
///
/// Principals are ranked by net realized `PnL` (the signed sum of their
/// settlement cashflows in the window) descending, using competition ranking
/// (ties share a rank). Pass `members` to rank within a league / affiliation
/// member set instead of the whole population — that set is ranked in
/// isolation and every listed member is included even with no settlements in
/// the window, so the front end can read a user's rank within their league.
///
/// Served from the `SETTLEMENT_LEADERBOARD` index, so a call ranks only the two
/// relevant period buckets (current + prior) rather than scanning the whole
/// event log. Guarded by `caller_is_not_anonymous`, matching the other
/// settlement-derived reads.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn list_leaderboard(params: ListLeaderboardParams) -> LeaderboardPage {
    SETTLEMENT_LEADERBOARD.with(|idx| list_leaderboard_impl(&idx.borrow(), params, now_ns()))
}

/// Aggregates each supplied principal's settled-position win/total counts (and
/// net realized `PnL`) over an arbitrary half-open window `[from_ts, to_ts)`.
///
/// `win_count / settled_count` per entry is that principal's window accuracy —
/// the same metric [`list_leaderboard`] exposes, but over a caller-chosen span
/// rather than a fixed calendar bucket. A consumer scoring a cohort (e.g. one
/// league side of a "battle") sums the entries it gets back for that side's
/// members.
///
/// Unlike [`list_leaderboard`], the arbitrary window cannot use the
/// `(window, period)` index, so this scans the raw `EVENTS` log once —
/// `O(events)`, the same cost class as [`get_trade_history`] and the
/// post-upgrade leaderboard rebuild. Only `Settled` events count; a `Settled`
/// event's `qty` is the position's signed `cashflow_usd`, so a win is
/// `qty > 0`, matching the leaderboard's rule exactly. Guarded by
/// `caller_is_not_anonymous`, matching the other settlement-derived reads.
///
/// [`get_trade_history`]: crate::api::trade::get_trade_history
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn aggregate_settlement_accuracy(
    params: AggregateSettlementAccuracyParams,
) -> Vec<SettlementAccuracyEntry> {
    EVENTS.with(|events| aggregate_settlement_accuracy_impl(&events.borrow(), params))
}

/// Storage-injectable core of [`aggregate_settlement_accuracy`]: folds the
/// supplied `events` slice so tests can pass a fixed log. Returns entries
/// ordered by principal for a deterministic response.
fn aggregate_settlement_accuracy_impl(
    events: &[Event],
    params: AggregateSettlementAccuracyParams,
) -> Vec<SettlementAccuracyEntry> {
    let AggregateSettlementAccuracyParams {
        members,
        from_ts,
        to_ts,
    } = params;

    let members: BTreeSet<User> = members
        .into_iter()
        .take(MAX_LEADERBOARD_MEMBERS)
        .map(User)
        .collect();

    if members.is_empty() {
        return Vec::new();
    }

    let mut acc: BTreeMap<User, PnlAggregate> = BTreeMap::new();
    for e in events {
        if matches!(e.event_type, EventType::Settled)
            && members.contains(&e.user)
            && from_ts.is_none_or(|from| e.timestamp >= from)
            && to_ts.is_none_or(|to| e.timestamp < to)
        {
            acc.entry(e.user).or_default().record(e.qty);
        }
    }

    acc.into_iter()
        .map(|(user, agg)| SettlementAccuracyEntry {
            principal: user.0,
            settled_count: agg.settled_count,
            win_count: agg.win_count,
            realized_pnl: agg.realized_pnl,
        })
        .collect()
}

/// Clock-injectable core of [`list_leaderboard`]. `now_ns` selects the current
/// period (and hence the prior one); the canister entry point passes the IC
/// time, tests pass a fixed instant.
fn list_leaderboard_impl(
    idx: &LeaderboardIndex,
    params: ListLeaderboardParams,
    now_ns: u64,
) -> LeaderboardPage {
    let ListLeaderboardParams {
        window,
        mut members,
        start_after,
        limit,
    } = params;
    // Bound the caller-supplied league filter before it drives any allocation
    // or sort (see `MAX_LEADERBOARD_MEMBERS`).
    if let Some(m) = members.as_mut() {
        m.truncate(MAX_LEADERBOARD_MEMBERS);
    }
    let members = members.as_deref();

    let mut current = collect_population(idx, window, window.period_id(now_ns), members);
    let total = current.len();

    // Pagination: `start_after` is the number of entries already consumed.
    let start = usize::try_from(start_after.unwrap_or(0))
        .unwrap_or(usize::MAX)
        .min(total);
    let limit = usize::try_from(limit.unwrap_or(u64::MAX))
        .unwrap_or(usize::MAX)
        .max(1);
    let end = start.saturating_add(limit).min(total);

    // Only the top `end` entries have to be ordered to serve the page and its
    // ranks: an entry's competition rank counts the strictly-higher `PnL`s, which
    // are themselves all within the top `end`. So partition the top `end` out
    // in O(total) and sort just that prefix — instead of sorting the whole
    // population on every call. `by_rank` is a strict total order (principals
    // are unique), so the top `end` is well-defined even across `PnL` ties.
    if end < total {
        current.select_nth_unstable_by(end, by_rank);
    }
    current[..end].sort_unstable_by(by_rank);
    let ranked = &current[..end];
    let ranks = competition_ranks(ranked);
    let page = &ranked[start..];

    // Prior period (if any): the page needs each shown principal's prior rank
    // for the ↑/↓ delta — not the whole prior ranking — so compute only those.
    let prior_ranks = window
        .prior_period_id(now_ns)
        .map(|period| prior_ranks_for(idx, window, period, members, page));

    let items = page
        .iter()
        .enumerate()
        .map(|(offset, (principal, agg))| LeaderboardEntry {
            principal: *principal,
            rank: ranks[start + offset],
            prior_rank: prior_ranks.as_ref().and_then(|m| m.get(principal).copied()),
            realized_pnl: agg.realized_pnl,
            settled_count: agg.settled_count,
            win_count: agg.win_count,
        })
        .collect();

    let next_cursor = (end < total).then_some(end as u64);

    LeaderboardPage {
        items,
        next_cursor,
        total: total as u64,
    }
}

/// Total order used to rank standings: realized `PnL` descending, ties broken by
/// principal ascending. Strict (principals are unique), so the top-`k` of a
/// population is uniquely defined — which is what lets a paged read select a
/// page without sorting the whole population.
fn by_rank(a: &(Principal, PnlAggregate), b: &(Principal, PnlAggregate)) -> Ordering {
    b.1.realized_pnl
        .cmp(&a.1.realized_pnl)
        .then_with(|| a.0.cmp(&b.0))
}

/// Collects the `(principal, aggregate)` pairs for one `(window, period)` bucket,
/// **unsorted**.
///
/// With `members = Some`, the bucket is restricted to that set and **every**
/// listed member is included — those absent from the bucket get a zeroed
/// aggregate — so a league ranking covers the full member set. Duplicate
/// members are de-duplicated. With `members = None`, only principals present in
/// the bucket (i.e. who settled at least one position in the period) are
/// returned.
fn collect_population(
    idx: &LeaderboardIndex,
    window: LeaderboardWindow,
    period: u64,
    members: Option<&[Principal]>,
) -> Vec<(Principal, PnlAggregate)> {
    let bucket = idx.get(&(window, period));

    match members {
        Some(members) => {
            let mut seen = BTreeSet::new();
            members
                .iter()
                .filter(|p| seen.insert(**p))
                .map(|p| {
                    let agg = bucket
                        .and_then(|b| b.get(&User(*p)))
                        .copied()
                        .unwrap_or_default();
                    (*p, agg)
                })
                .collect()
        }
        None => bucket
            .map(|b| b.iter().map(|(u, a)| (u.principal(), *a)).collect())
            .unwrap_or_default(),
    }
}

/// Assigns 1-based competition ranks to a [`by_rank`]-sorted slice: equal-`PnL`
/// runs share the rank of their first element, and the next distinct `PnL`
/// takes the ordinal position (e.g. `PnL`s `100, 50, 50, 10` → ranks
/// `1, 2, 2, 4`). Returns one rank per input entry, in input order.
fn competition_ranks(sorted: &[(Principal, PnlAggregate)]) -> Vec<u64> {
    let mut ranks = Vec::with_capacity(sorted.len());
    let mut last_pnl: Option<i128> = None;
    let mut last_rank: u64 = 0;
    for (position, (_, agg)) in sorted.iter().enumerate() {
        let rank = if last_pnl == Some(agg.realized_pnl) {
            last_rank
        } else {
            position as u64 + 1
        };
        ranks.push(rank);
        last_pnl = Some(agg.realized_pnl);
        last_rank = rank;
    }
    ranks
}

/// Prior-period rank for each principal on `page`, keyed by principal. A
/// principal absent from the prior period is omitted (→ `None` prior rank).
///
/// For a league query (`members = Some`) the population is bounded by the
/// member set, so it is simply ranked in full. For a global query the prior
/// bucket can be large, so rather than rank the whole thing we compute the rank
/// of just the shown principals: a competition rank is `1 + count(strictly
/// higher PnL)`, which one linear pass over the bucket tallies for all of them
/// at once (see [`global_prior_ranks`]).
fn prior_ranks_for(
    idx: &LeaderboardIndex,
    window: LeaderboardWindow,
    period: u64,
    members: Option<&[Principal]>,
    page: &[(Principal, PnlAggregate)],
) -> HashMap<Principal, u64> {
    match members {
        Some(members) => {
            let mut prior = collect_population(idx, window, period, Some(members));
            prior.sort_unstable_by(by_rank);
            let ranks = competition_ranks(&prior);
            prior
                .into_iter()
                .map(|(principal, _)| principal)
                .zip(ranks)
                .collect()
        }
        None => global_prior_ranks(idx.get(&(window, period)), page),
    }
}

/// Computes the prior competition rank of each principal on `page` against the
/// full (global) prior `bucket`, without sorting the bucket.
///
/// A competition rank is `1 + count(entries with strictly higher PnL)`. We tally
/// that for every shown principal in a single O(bucket · log page) pass: with
/// the shown principals' distinct prior `PnL`s sorted ascending as `targets`, each
/// bucket entry with `PnL` `v` contributes to exactly the targets it exceeds — the
/// prefix `[0, j)` where `j` is the number of targets below `v` — accumulated as
/// a range increment and resolved by a prefix sum.
fn global_prior_ranks(
    bucket: Option<&BTreeMap<User, PnlAggregate>>,
    page: &[(Principal, PnlAggregate)],
) -> HashMap<Principal, u64> {
    let Some(bucket) = bucket else {
        return HashMap::new();
    };

    // Prior `PnL` of each shown principal that actually appears in the prior bucket.
    let present: Vec<(Principal, i128)> = page
        .iter()
        .filter_map(|(p, _)| bucket.get(&User(*p)).map(|agg| (*p, agg.realized_pnl)))
        .collect();
    if present.is_empty() {
        return HashMap::new();
    }

    // Distinct shown `PnL`s, ascending, so a bucket `PnL` maps to the targets it
    // exceeds via `partition_point`.
    let mut targets: Vec<i128> = present.iter().map(|(_, pnl)| *pnl).collect();
    targets.sort_unstable();
    targets.dedup();

    // `diff` is a range-increment buffer; after the pass, its prefix sum at `i`
    // is the number of bucket entries with `PnL` strictly greater than `targets[i]`.
    let mut diff = vec![0_i64; targets.len() + 1];
    for agg in bucket.values() {
        let j = targets.partition_point(|&t| t < agg.realized_pnl);
        if j > 0 {
            diff[0] += 1;
            diff[j] -= 1;
        }
    }
    let mut greater = vec![0_i64; targets.len()];
    let mut running = 0_i64;
    for (slot, d) in greater.iter_mut().zip(&diff) {
        running += d;
        *slot = running;
    }

    present
        .into_iter()
        .map(|(principal, pnl)| {
            let i = targets.partition_point(|&t| t < pnl);
            let rank = u64::try_from(greater[i] + 1).unwrap_or(1);
            (principal, rank)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use shared::types::Price;

    use super::*;
    use crate::memory::index_settled_events;

    const DAY_NS: u64 = 86_400_000_000_000;

    fn principal(n: u8) -> Principal {
        Principal::from_slice(&[n])
    }

    fn agg(pnl: i128, settled: u64, wins: u64) -> PnlAggregate {
        PnlAggregate {
            realized_pnl: pnl,
            settled_count: settled,
            win_count: wins,
        }
    }

    /// Replaces the leaderboard index with the supplied `(window, period,
    /// principal, aggregate)` rows.
    fn set_index(rows: Vec<(LeaderboardWindow, u64, Principal, PnlAggregate)>) {
        SETTLEMENT_LEADERBOARD.with(|idx| {
            let mut idx = idx.borrow_mut();
            idx.clear();
            for (window, period, p, a) in rows {
                idx.entry((window, period)).or_default().insert(User(p), a);
            }
        });
    }

    fn query(params: ListLeaderboardParams, now_ns: u64) -> LeaderboardPage {
        SETTLEMENT_LEADERBOARD.with(|idx| list_leaderboard_impl(&idx.borrow(), params, now_ns))
    }

    fn params(window: LeaderboardWindow) -> ListLeaderboardParams {
        ListLeaderboardParams {
            window,
            members: None,
            start_after: None,
            limit: None,
        }
    }

    #[test]
    fn ranks_by_pnl_descending_with_competition_ties() {
        let now = DAY_NS * 100;
        let period = LeaderboardWindow::Week.period_id(now);
        set_index(vec![
            (
                LeaderboardWindow::Week,
                period,
                principal(1),
                agg(100, 4, 3),
            ),
            (LeaderboardWindow::Week, period, principal(2), agg(50, 2, 1)),
            (LeaderboardWindow::Week, period, principal(3), agg(50, 5, 2)),
            (LeaderboardWindow::Week, period, principal(4), agg(10, 1, 0)),
        ]);

        let page = query(params(LeaderboardWindow::Week), now);

        assert_eq!(page.total, 4);
        let ranks: Vec<(Principal, u64)> =
            page.items.iter().map(|e| (e.principal, e.rank)).collect();
        // 100, 50, 50, 10 → 1, 2, 2, 4; the tied 50s ordered by principal asc.
        assert_eq!(
            ranks,
            vec![
                (principal(1), 1),
                (principal(2), 2),
                (principal(3), 2),
                (principal(4), 4),
            ]
        );
        // Aggregate fields are surfaced for the accuracy figure.
        assert_eq!(page.items[0].settled_count, 4);
        assert_eq!(page.items[0].win_count, 3);
        assert_eq!(page.items[0].realized_pnl, 100);
    }

    #[test]
    fn prior_rank_is_populated_from_preceding_period() {
        let now = DAY_NS * 100;
        let period = LeaderboardWindow::Week.period_id(now);
        let prior = period - 1;
        set_index(vec![
            // Current week: p1 leads, p2 second.
            (
                LeaderboardWindow::Week,
                period,
                principal(1),
                agg(100, 1, 1),
            ),
            (LeaderboardWindow::Week, period, principal(2), agg(40, 1, 1)),
            // Prior week: order was reversed (p2 first, p1 second).
            (LeaderboardWindow::Week, prior, principal(1), agg(10, 1, 1)),
            (LeaderboardWindow::Week, prior, principal(2), agg(90, 1, 1)),
        ]);

        let page = query(params(LeaderboardWindow::Week), now);

        let p1 = &page.items[0];
        let p2 = &page.items[1];
        assert_eq!(
            (p1.principal, p1.rank, p1.prior_rank),
            (principal(1), 1, Some(2))
        );
        assert_eq!(
            (p2.principal, p2.rank, p2.prior_rank),
            (principal(2), 2, Some(1))
        );
    }

    #[test]
    fn prior_rank_none_when_absent_in_prior_period() {
        let now = DAY_NS * 100;
        let period = LeaderboardWindow::Month.period_id(now);
        set_index(vec![(
            LeaderboardWindow::Month,
            period,
            principal(1),
            agg(100, 1, 1),
        )]);

        let page = query(params(LeaderboardWindow::Month), now);
        // Newcomer this month: ranked, but no prior-month rank.
        assert_eq!(page.items[0].prior_rank, None);
    }

    #[test]
    fn all_time_window_has_no_prior_rank() {
        let now = DAY_NS * 100;
        set_index(vec![
            (LeaderboardWindow::AllTime, 0, principal(1), agg(100, 1, 1)),
            (LeaderboardWindow::AllTime, 0, principal(2), agg(50, 1, 1)),
        ]);

        let page = query(params(LeaderboardWindow::AllTime), now);
        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|e| e.prior_rank.is_none()));
    }

    #[test]
    fn members_filter_ranks_within_league_and_includes_inactive_members() {
        let now = DAY_NS * 100;
        let period = LeaderboardWindow::AllTime.period_id(now);
        set_index(vec![
            // A non-member outranks everyone globally but must be excluded.
            (
                LeaderboardWindow::AllTime,
                period,
                principal(9),
                agg(1000, 9, 9),
            ),
            (
                LeaderboardWindow::AllTime,
                period,
                principal(1),
                agg(100, 2, 2),
            ),
            (
                LeaderboardWindow::AllTime,
                period,
                principal(2),
                agg(30, 1, 1),
            ),
        ]);

        let mut p = params(LeaderboardWindow::AllTime);
        // principal(3) is a league member who has never settled → zeroed.
        p.members = Some(vec![principal(1), principal(2), principal(3)]);
        let page = query(p.clone(), now);

        assert_eq!(page.total, 3);
        let view: Vec<(Principal, u64, i128, u64)> = page
            .items
            .iter()
            .map(|e| (e.principal, e.rank, e.realized_pnl, e.settled_count))
            .collect();
        assert_eq!(
            view,
            vec![
                (principal(1), 1, 100, 2),
                (principal(2), 2, 30, 1),
                (principal(3), 3, 0, 0),
            ]
        );
        // The high-`PnL` non-member never appears.
        assert!(page.items.iter().all(|e| e.principal != principal(9)));
    }

    #[test]
    fn pagination_is_stable_and_clamps_zero_limit() {
        let now = DAY_NS * 100;
        let period = LeaderboardWindow::Week.period_id(now);
        set_index(vec![
            (LeaderboardWindow::Week, period, principal(1), agg(40, 1, 1)),
            (LeaderboardWindow::Week, period, principal(2), agg(30, 1, 1)),
            (LeaderboardWindow::Week, period, principal(3), agg(20, 1, 1)),
            (LeaderboardWindow::Week, period, principal(4), agg(10, 1, 1)),
        ]);

        let mut p = params(LeaderboardWindow::Week);
        p.limit = Some(2);
        let page1 = query(p.clone(), now);
        assert_eq!(
            page1.items.iter().map(|e| e.principal).collect::<Vec<_>>(),
            vec![principal(1), principal(2)]
        );
        assert_eq!(page1.next_cursor, Some(2));

        p.start_after = page1.next_cursor;
        let page2 = query(p.clone(), now);
        assert_eq!(
            page2.items.iter().map(|e| e.principal).collect::<Vec<_>>(),
            vec![principal(3), principal(4)]
        );
        assert_eq!(page2.next_cursor, None);

        // limit = 0 is clamped to 1 so a caller still makes forward progress.
        let mut zero = params(LeaderboardWindow::Week);
        zero.limit = Some(0);
        let page = query(zero, now);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.next_cursor, Some(1));
    }

    #[test]
    fn empty_or_unknown_window_returns_empty_page() {
        set_index(vec![]);
        let page = query(params(LeaderboardWindow::Week), DAY_NS * 100);
        assert_eq!(page, LeaderboardPage::default());
    }

    #[test]
    fn index_buckets_settled_events_by_window_via_write_path() {
        // Exercise the maintenance path: feed Settled events and confirm the
        // query reads them back, bucketed by the event timestamp's window.
        let now = DAY_NS * 100;
        let prior_week_ts = now - DAY_NS * 8; // a week earlier → prior week bucket

        let settled = |p: u8, qty: i128, ts: u64| Event {
            event_id: 0,
            clearing_id: Principal::anonymous(),
            series_id: "s".to_owned().into(),
            user: User(principal(p)),
            qty,
            price: Price::new(1, 0),
            event_type: EventType::Settled,
            timestamp: ts,
        };
        // A non-settled event must be ignored by the index.
        let mut order = settled(1, 999, now);
        order.event_type = EventType::Executed;

        SETTLEMENT_LEADERBOARD.with(|idx| idx.borrow_mut().clear());
        index_settled_events(&[
            settled(1, 100, now),
            settled(1, -30, now), // same user, same week → folds together
            settled(2, 50, now),
            settled(1, 70, prior_week_ts), // prior week
            order,
        ]);

        let page = query(params(LeaderboardWindow::Week), now);
        assert_eq!(page.total, 2);
        // p1 current week: 100 - 30 = 70 over 2 settlements, 1 win.
        let p1 = page
            .items
            .iter()
            .find(|e| e.principal == principal(1))
            .unwrap();
        assert_eq!(
            (p1.realized_pnl, p1.settled_count, p1.win_count),
            (70, 2, 1)
        );
        // p1's prior-week aggregate (70) ranked it #1 there.
        assert_eq!(p1.prior_rank, Some(1));

        // All-time folds every week together: p1 = 70 + 70 = 140.
        let all = query(params(LeaderboardWindow::AllTime), now);
        let p1_all = all
            .items
            .iter()
            .find(|e| e.principal == principal(1))
            .unwrap();
        assert_eq!(p1_all.realized_pnl, 140);
        assert_eq!(p1_all.settled_count, 3);
    }

    #[test]
    fn members_filter_is_capped() {
        set_index(vec![]);
        // A members list longer than the cap is truncated before it drives any
        // ranking work, so the result covers at most `MAX_LEADERBOARD_MEMBERS`.
        let oversized = u32::try_from(MAX_LEADERBOARD_MEMBERS).unwrap() + 5;
        let members: Vec<Principal> = (0..oversized)
            .map(|i| Principal::from_slice(&i.to_be_bytes()))
            .collect();
        let mut p = params(LeaderboardWindow::AllTime);
        p.members = Some(members);
        let page = query(p, DAY_NS * 100);
        assert_eq!(page.total, MAX_LEADERBOARD_MEMBERS as u64);
    }

    /// Reference implementation: rank the whole population (and the whole prior
    /// period) with a full sort, then page. Obviously correct, and what the
    /// optimized `list_leaderboard_impl` must match byte-for-byte.
    fn naive_impl(
        idx: &LeaderboardIndex,
        params: ListLeaderboardParams,
        now_ns: u64,
    ) -> LeaderboardPage {
        let ListLeaderboardParams {
            window,
            members,
            start_after,
            limit,
        } = params;
        let members = members.as_deref();

        let mut current = collect_population(idx, window, window.period_id(now_ns), members);
        let total = current.len();
        current.sort_by(by_rank);
        let ranks = competition_ranks(&current);

        let prior_ranks: Option<HashMap<Principal, u64>> =
            window.prior_period_id(now_ns).map(|period| {
                let mut prior = collect_population(idx, window, period, members);
                prior.sort_by(by_rank);
                let r = competition_ranks(&prior);
                prior
                    .into_iter()
                    .map(|(principal, _)| principal)
                    .zip(r)
                    .collect()
            });

        let start = usize::try_from(start_after.unwrap_or(0))
            .unwrap_or(usize::MAX)
            .min(total);
        let limit = usize::try_from(limit.unwrap_or(u64::MAX))
            .unwrap_or(usize::MAX)
            .max(1);
        let end = start.saturating_add(limit).min(total);

        let items = current[start..end]
            .iter()
            .enumerate()
            .map(|(offset, (principal, agg))| LeaderboardEntry {
                principal: *principal,
                rank: ranks[start + offset],
                prior_rank: prior_ranks.as_ref().and_then(|m| m.get(principal).copied()),
                realized_pnl: agg.realized_pnl,
                settled_count: agg.settled_count,
                win_count: agg.win_count,
            })
            .collect();

        let next_cursor = (end < total).then_some(end as u64);
        LeaderboardPage {
            items,
            next_cursor,
            total: total as u64,
        }
    }

    #[test]
    fn optimized_matches_naive_reference_over_random_inputs() {
        // xorshift64 — a deterministic PRNG so failures reproduce without a dep.
        let mut state = 0x1234_5678_9abc_def0_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        let windows = [
            LeaderboardWindow::Week,
            LeaderboardWindow::Month,
            LeaderboardWindow::AllTime,
        ];

        for _ in 0..600 {
            let window = windows[(next() % 3) as usize];
            // Vary `now` across periods so prior is sometimes empty/at boundary.
            let now = DAY_NS * (60 + next() % 400);
            let cur = window.period_id(now);
            let prior = window.prior_period_id(now);

            // Up to 14 principals, each maybe in the current and/or prior bucket
            // with a small `PnL` drawn from a tight range to force frequent ties.
            let n_principals = 1 + (next() % 14) as u8;
            let mut rows = Vec::new();
            let pnl = |r: u64| i128::from(r % 7) - 3; // -3..=3
            for n in 1..=n_principals {
                if next() % 4 != 0 {
                    rows.push((
                        window,
                        cur,
                        principal(n),
                        agg(pnl(next()), 1 + next() % 3, 0),
                    ));
                }
                if let Some(prior) = prior {
                    if next() % 4 != 0 {
                        rows.push((window, prior, principal(n), agg(pnl(next()), 1, 0)));
                    }
                }
            }
            set_index(rows);

            // Random params: maybe a league filter (subset + a duplicate + an
            // absent principal), a random cursor, and a small/None/zero limit.
            let members = if next() % 3 == 0 {
                None
            } else {
                let mut m = Vec::new();
                for n in 1..=n_principals {
                    if next() % 2 == 0 {
                        m.push(principal(n));
                    }
                }
                if next() % 2 == 0 && !m.is_empty() {
                    m.push(m[0]); // duplicate
                }
                m.push(principal(200)); // never-settled member
                Some(m)
            };
            let start_after = if next() % 4 == 0 {
                None
            } else {
                Some(next() % (u64::from(n_principals) + 2))
            };
            let limit = match next() % 4 {
                0 => None,
                1 => Some(0),
                _ => Some(1 + next() % 5),
            };
            let params = ListLeaderboardParams {
                window,
                members,
                start_after,
                limit,
            };

            let (optimized, reference) = SETTLEMENT_LEADERBOARD.with(|idx| {
                let idx = idx.borrow();
                (
                    list_leaderboard_impl(&idx, params.clone(), now),
                    naive_impl(&idx, params, now),
                )
            });
            assert_eq!(optimized, reference);
        }
    }

    #[test]
    fn aggregate_settlement_accuracy_folds_members_over_half_open_window() {
        let event = |p: u8, qty: i128, ts: u64, kind: EventType| Event {
            event_id: 0,
            clearing_id: Principal::anonymous(),
            series_id: "s".to_owned().into(),
            user: User(principal(p)),
            qty,
            price: Price::new(1, 0),
            event_type: kind,
            timestamp: ts,
        };

        let events = vec![
            event(1, 100, 1_000, EventType::Settled),  // in window, win
            event(1, -40, 2_000, EventType::Settled),  // in window, loss
            event(2, 0, 1_500, EventType::Settled),    // in window, break-even (not a win)
            event(1, 999, 5_000, EventType::Settled),  // == to_ts → excluded (half-open)
            event(1, 999, 500, EventType::Settled),    // < from_ts → excluded
            event(1, 999, 1_200, EventType::Executed), // not a settlement → excluded
            event(3, 999, 1_200, EventType::Settled),  // not a listed member → excluded
        ];

        let out = aggregate_settlement_accuracy_impl(
            &events,
            AggregateSettlementAccuracyParams {
                members: vec![principal(1), principal(2)],
                from_ts: Some(1_000),
                to_ts: Some(5_000),
            },
        );

        let p1 = out.iter().find(|e| e.principal == principal(1)).unwrap();
        assert_eq!(
            (p1.settled_count, p1.win_count, p1.realized_pnl),
            (2, 1, 60) // 100 + (-40), one net-positive
        );
        let p2 = out.iter().find(|e| e.principal == principal(2)).unwrap();
        assert_eq!((p2.settled_count, p2.win_count, p2.realized_pnl), (1, 0, 0));
        // A non-member settled in-window is never folded in.
        assert!(out.iter().all(|e| e.principal != principal(3)));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn aggregate_settlement_accuracy_unbounded_window_and_empty_members() {
        let settled = |p: u8, qty: i128, ts: u64| Event {
            event_id: 0,
            clearing_id: Principal::anonymous(),
            series_id: "s".to_owned().into(),
            user: User(principal(p)),
            qty,
            price: Price::new(1, 0),
            event_type: EventType::Settled,
            timestamp: ts,
        };
        let events = vec![settled(1, 10, 1), settled(1, -5, 9_999_999)];

        // No bounds → every settlement in the log counts.
        let all = aggregate_settlement_accuracy_impl(
            &events,
            AggregateSettlementAccuracyParams {
                members: vec![principal(1)],
                from_ts: None,
                to_ts: None,
            },
        );
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].settled_count, all[0].win_count), (2, 1));

        // Empty member set → empty result without scanning.
        let none = aggregate_settlement_accuracy_impl(
            &events,
            AggregateSettlementAccuracyParams {
                members: vec![],
                from_ts: None,
                to_ts: None,
            },
        );
        assert!(none.is_empty());
    }
}
