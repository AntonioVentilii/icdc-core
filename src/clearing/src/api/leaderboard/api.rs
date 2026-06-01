use std::collections::{BTreeMap, BTreeSet, HashMap};

use candid::Principal;
use ic_cdk_macros::query;

use super::{
    params::ListLeaderboardParams,
    results::{LeaderboardEntry, LeaderboardPage},
};
use crate::{
    guards::caller_is_not_anonymous,
    memory::SETTLEMENT_LEADERBOARD,
    types::{
        leaderboard::{LeaderboardWindow, PnlAggregate},
        user::User,
    },
    utils::system::now_ns,
};

/// The maintained leaderboard index shape: realized `PnL` aggregated per principal
/// within each `(window, period)` bucket.
type LeaderboardIndex = BTreeMap<(LeaderboardWindow, u64), BTreeMap<User, PnlAggregate>>;

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
        members,
        start_after,
        limit,
    } = params;
    let members = members.as_deref();

    // Current period: collect, order deterministically, and rank.
    let current = ranked_period(idx, window, window.period_id(now_ns), members);
    let current_ranks = competition_ranks(&current);

    // Prior period (if any): rank it too, but keyed by principal for lookup,
    // since the displayed page is the current period's order.
    let prior_ranks: Option<HashMap<Principal, u64>> = window.prior_period_id(now_ns).map(|p| {
        let prior = ranked_period(idx, window, p, members);
        let ranks = competition_ranks(&prior);
        prior
            .iter()
            .map(|(principal, _)| *principal)
            .zip(ranks)
            .collect()
    });

    let total = current.len();

    // Pagination: `start_after` is the number of entries already consumed.
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
            rank: current_ranks[start + offset],
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

/// Collects the `(principal, aggregate)` pairs for one `(window, period)` bucket
/// in display order: realized `PnL` descending, ties broken by principal ascending
/// for a stable, deterministic ordering.
///
/// With `members = Some`, the bucket is restricted to that set and **every**
/// listed member is included — those absent from the bucket get a zeroed
/// aggregate — so a league ranking covers the full member set. Duplicate
/// members are de-duplicated. With `members = None`, only principals present in
/// the bucket (i.e. who settled at least one position in the period) are
/// returned.
fn ranked_period(
    idx: &LeaderboardIndex,
    window: LeaderboardWindow,
    period: u64,
    members: Option<&[Principal]>,
) -> Vec<(Principal, PnlAggregate)> {
    let bucket = idx.get(&(window, period));

    let mut entries: Vec<(Principal, PnlAggregate)> = match members {
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
    };

    entries.sort_by(|a, b| {
        b.1.realized_pnl
            .cmp(&a.1.realized_pnl)
            .then_with(|| a.0.cmp(&b.0))
    });
    entries
}

/// Assigns 1-based competition ranks to an already-sorted (`PnL` descending)
/// slice: equal-`PnL` runs share the rank of their first element, and the next
/// distinct `PnL` takes the ordinal position (e.g. `PnL`s `100, 50, 50, 10` →
/// ranks `1, 2, 2, 4`). Returns one rank per input entry, in input order.
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

#[cfg(test)]
mod tests {
    use shared::types::Price;

    use super::*;
    use crate::{
        memory::index_settled_events,
        types::event::{Event, EventType},
    };

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
}
