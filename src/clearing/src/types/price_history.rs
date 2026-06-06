use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Nanoseconds in one hour and one day (UTC). Bucket widths for the
/// price-history aggregation below.
const NANOS_PER_HOUR: u64 = 3_600_000_000_000;
const NANOS_PER_DAY: u64 = 86_400_000_000_000;

/// The fixed-width time bucket a [`get_series_price_history`] call aggregates a
/// series' executed trades into.
///
/// Buckets are **fixed-width and epoch-aligned** (each bucket is `[k * width,
/// (k + 1) * width)` ns since the Unix epoch), not rolling spans, so the same
/// instant always falls in the same bucket regardless of when the query runs —
/// two calls over an overlapping range return byte-identical candles for the
/// shared buckets. Hourly resolution backs the short windows the front end
/// renders (1d / 7d) and daily resolution the long ones (30d / all) without the
/// caller having to fetch and re-bucket the raw per-trade tape.
///
/// [`get_series_price_history`]: crate::api::trade::get_series_price_history
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceHistoryInterval {
    /// One candle per UTC hour.
    Hour,
    /// One candle per UTC day.
    Day,
}

impl PriceHistoryInterval {
    /// Width of one bucket in nanoseconds.
    #[must_use]
    pub fn bucket_ns(self) -> u64 {
        match self {
            PriceHistoryInterval::Hour => NANOS_PER_HOUR,
            PriceHistoryInterval::Day => NANOS_PER_DAY,
        }
    }

    /// Start (ns since the Unix epoch) of the bucket containing `ts_ns`. Two
    /// timestamps share a bucket iff they map to the same start, so this both
    /// keys and labels a candle.
    #[must_use]
    pub fn bucket_start(self, ts_ns: u64) -> u64 {
        let width = self.bucket_ns();
        (ts_ns / width) * width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hour_buckets_align_to_the_hour() {
        let h = NANOS_PER_HOUR;
        // Anything within the same hour shares the hour's start.
        assert_eq!(PriceHistoryInterval::Hour.bucket_start(0), 0);
        assert_eq!(PriceHistoryInterval::Hour.bucket_start(h - 1), 0);
        assert_eq!(PriceHistoryInterval::Hour.bucket_start(h), h);
        assert_eq!(PriceHistoryInterval::Hour.bucket_start(h + 5), h);
        assert_eq!(PriceHistoryInterval::Hour.bucket_start(3 * h + 7), 3 * h);
    }

    #[test]
    fn day_buckets_align_to_the_day() {
        let d = NANOS_PER_DAY;
        assert_eq!(PriceHistoryInterval::Day.bucket_start(0), 0);
        assert_eq!(PriceHistoryInterval::Day.bucket_start(d - 1), 0);
        assert_eq!(PriceHistoryInterval::Day.bucket_start(d), d);
        assert_eq!(PriceHistoryInterval::Day.bucket_start(2 * d + d / 2), 2 * d);
    }

    #[test]
    fn bucket_width_matches_resolution() {
        assert_eq!(PriceHistoryInterval::Hour.bucket_ns(), 3_600_000_000_000);
        assert_eq!(PriceHistoryInterval::Day.bucket_ns(), 86_400_000_000_000);
    }
}
