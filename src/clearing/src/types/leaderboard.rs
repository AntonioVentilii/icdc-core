use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Nanoseconds in a calendar day (UTC). Used to bucket settlement timestamps
/// into the calendar windows below.
const NANOS_PER_DAY: u64 = 86_400_000_000_000;

/// The time window a leaderboard aggregates over.
///
/// Windows are **fixed calendar periods**, not rolling spans, so each window
/// has a well-defined immediately-preceding period (the "prior window") for the
/// ↑/↓ rank deltas the front end renders:
///
/// - [`Week`](LeaderboardWindow::Week): an ISO week — Monday 00:00:00 UTC through the following
///   Sunday. (Unix epoch day 0, 1970-01-01, was a Thursday, hence the `+ 3` Monday alignment
///   below.)
/// - [`Month`](LeaderboardWindow::Month): a calendar month in UTC.
/// - [`AllTime`](LeaderboardWindow::AllTime): a single, unbounded period covering every settlement.
///   Has no prior window.
///
/// Calendar (rather than rolling) buckets were chosen because the prior-window
/// rank only has an unambiguous meaning against a discrete preceding period,
/// and because fixed buckets let the [`SETTLEMENT_LEADERBOARD`] index key on a
/// stable `(window, period_id)` pair maintained on the settlement write path.
///
/// [`SETTLEMENT_LEADERBOARD`]: crate::memory::SETTLEMENT_LEADERBOARD
#[derive(
    CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum LeaderboardWindow {
    /// Current ISO week (Monday–Sunday, UTC).
    Week,
    /// Current calendar month (UTC).
    Month,
    /// All settlements ever, in one period.
    AllTime,
}

impl LeaderboardWindow {
    /// Every window kind, so the index can bucket each settlement into all of
    /// them in one pass.
    pub const ALL: [LeaderboardWindow; 3] = [
        LeaderboardWindow::Week,
        LeaderboardWindow::Month,
        LeaderboardWindow::AllTime,
    ];

    /// Maps a timestamp (ns since the Unix epoch) to the id of the period that
    /// contains it for this window. Period ids are monotonically increasing in
    /// time, so the period immediately before `id` is `id - 1` (see
    /// [`prior_period_id`](LeaderboardWindow::prior_period_id)).
    #[must_use]
    pub fn period_id(self, ts_ns: u64) -> u64 {
        let days = ts_ns / NANOS_PER_DAY;
        match self {
            // Epoch day 0 is a Thursday; shifting by 3 days aligns the integer
            // division to Monday-started weeks.
            LeaderboardWindow::Week => (days + 3) / 7,
            LeaderboardWindow::Month => {
                let (year, month) = civil_from_days(days);
                // A monotonic month index; the absolute value is irrelevant, it
                // only has to increase by exactly one each calendar month.
                year * 12 + u64::from(month - 1)
            }
            LeaderboardWindow::AllTime => 0,
        }
    }

    /// The id of the period immediately preceding the one containing `ts_ns`,
    /// or `None` when there is no prior window ([`AllTime`], or a period at the
    /// very start of the epoch).
    ///
    /// [`AllTime`]: LeaderboardWindow::AllTime
    #[must_use]
    pub fn prior_period_id(self, ts_ns: u64) -> Option<u64> {
        match self {
            LeaderboardWindow::AllTime => None,
            _ => self.period_id(ts_ns).checked_sub(1),
        }
    }
}

/// Per-principal aggregate of realized settlement `PnL` within one
/// `(window, period)` bucket. This is the maintained index value, not a Candid
/// type — the query projects it into a [`LeaderboardEntry`].
///
/// [`LeaderboardEntry`]: crate::api::leaderboard::results::LeaderboardEntry
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PnlAggregate {
    /// Net realized `PnL` over the period, in internal USD (`vUSD`) base units —
    /// the signed sum of each settlement's `cashflow_usd`.
    pub realized_pnl: i128,
    /// Number of settled positions contributing to this aggregate.
    pub settled_count: u64,
    /// Number of those settlements that were net positive (`cashflow_usd > 0`).
    /// Lets a front end derive an accuracy / win-rate without exposing the
    /// per-settlement breakdown.
    pub win_count: u64,
}

impl PnlAggregate {
    /// Folds one settlement's signed cashflow into the aggregate.
    pub fn record(&mut self, cashflow_usd: i128) {
        self.realized_pnl += cashflow_usd;
        self.settled_count += 1;
        if cashflow_usd > 0 {
            self.win_count += 1;
        }
    }
}

/// Civil calendar date (year, month) from a count of days since the Unix epoch
/// (1970-01-01), UTC. Day-of-month is not needed for monthly bucketing, so it
/// is discarded.
///
/// Howard Hinnant's `civil_from_days`
/// (<http://howardhinnant.github.io/date_algorithms.html#civil_from_days>),
/// specialized to the non-negative epoch days a canister timestamp produces.
fn civil_from_days(days: u64) -> (u64, u32) {
    // Shift epoch to 0000-03-01 so leap days fall at the end of the 400-year era.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097; // day of era, [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], months shifted so March = 0
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
                                                       // March–December belong to `year`; January/February roll into the next.
    let year = if month <= 2 { year + 1 } else { year };
    (year, u32::try_from(month).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Days (in ns) for a known UTC date, to drive the calendar math.
    fn ns_for(days: u64) -> u64 {
        days * NANOS_PER_DAY
    }

    #[test]
    fn week_buckets_align_to_monday() {
        // 1970-01-01 is a Thursday (epoch day 0). The ISO week containing it
        // starts Monday 1969-12-29, so days 0..=3 (Thu–Sun) share week 0, and
        // 1970-01-05 (Monday, day 4) opens week 1.
        for day in 0..=3 {
            assert_eq!(LeaderboardWindow::Week.period_id(ns_for(day)), 0);
        }
        for day in 4..=10 {
            assert_eq!(LeaderboardWindow::Week.period_id(ns_for(day)), 1);
        }
        assert_eq!(LeaderboardWindow::Week.period_id(ns_for(11)), 2);
    }

    #[test]
    fn week_sub_day_resolution_stays_in_bucket() {
        // Any instant within a calendar day maps to that day's week.
        let almost_midnight = ns_for(4) + (NANOS_PER_DAY - 1);
        assert_eq!(LeaderboardWindow::Week.period_id(almost_midnight), 1);
    }

    #[test]
    fn month_buckets_match_calendar_months() {
        // 1970-01-01 .. 1970-01-31 → same month; 1970-02-01 → next.
        let jan = LeaderboardWindow::Month.period_id(ns_for(0));
        let jan_end = LeaderboardWindow::Month.period_id(ns_for(30)); // 1970-01-31
        let feb = LeaderboardWindow::Month.period_id(ns_for(31)); // 1970-02-01
        assert_eq!(jan, jan_end);
        assert_eq!(feb, jan + 1);
    }

    #[test]
    fn month_index_increments_across_year_boundary() {
        // 1970-12-31 is epoch day 364; 1971-01-01 is day 365.
        let dec_1970 = LeaderboardWindow::Month.period_id(ns_for(364));
        let jan_1971 = LeaderboardWindow::Month.period_id(ns_for(365));
        assert_eq!(jan_1971, dec_1970 + 1);
    }

    #[test]
    fn month_handles_leap_year() {
        // 2020 is a leap year. 2020-02-29 and 2020-03-01 are distinct months.
        // Epoch days: 2020-02-29 = 18321, 2020-03-01 = 18322.
        let feb_29 = LeaderboardWindow::Month.period_id(ns_for(18_321));
        let mar_01 = LeaderboardWindow::Month.period_id(ns_for(18_322));
        assert_eq!(mar_01, feb_29 + 1);
        // And the day before is still February.
        let feb_28 = LeaderboardWindow::Month.period_id(ns_for(18_320));
        assert_eq!(feb_28, feb_29);
    }

    #[test]
    fn all_time_is_single_period_without_prior() {
        assert_eq!(LeaderboardWindow::AllTime.period_id(0), 0);
        assert_eq!(LeaderboardWindow::AllTime.period_id(ns_for(99_999)), 0);
        assert_eq!(
            LeaderboardWindow::AllTime.prior_period_id(ns_for(99_999)),
            None
        );
    }

    #[test]
    fn prior_period_is_one_less() {
        let ts = ns_for(10_000);
        assert_eq!(
            LeaderboardWindow::Week.prior_period_id(ts),
            Some(LeaderboardWindow::Week.period_id(ts) - 1)
        );
        assert_eq!(
            LeaderboardWindow::Month.prior_period_id(ts),
            Some(LeaderboardWindow::Month.period_id(ts) - 1)
        );
    }

    #[test]
    fn prior_week_at_epoch_start_is_none() {
        // Week 0 has no representable prior week.
        assert_eq!(LeaderboardWindow::Week.prior_period_id(0), None);
    }

    #[test]
    fn aggregate_records_wins_and_losses() {
        let mut agg = PnlAggregate::default();
        agg.record(100);
        agg.record(-40);
        agg.record(0); // break-even is not a win
        assert_eq!(
            agg,
            PnlAggregate {
                realized_pnl: 60,
                settled_count: 3,
                win_count: 1,
            }
        );
    }
}
