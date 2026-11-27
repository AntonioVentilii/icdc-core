//! Calendar: day weights and scheduled events (`docs/risk/margin-model.md`, section 6.3).

use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::risk::fixed::Fixed;

/// Nanoseconds in one UTC day.
pub const NS_PER_DAY: u64 = 86_400 * 1_000_000_000;

/// Maximum number of calendar events per underlying.
pub const MAX_EVENTS_PER_UNDERLYING: usize = 512;

/// Maximum length of an event label, in characters.
pub const MAX_LABEL_LEN: usize = 64;

/// A scheduled event whose day carries `weight` times the variance of an ordinary day.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CalendarEvent {
    pub id: u64,
    pub time_ns: u64,
    pub weight: Fixed,
    pub label: String,
}

/// Days since the Unix epoch (UTC) of a timestamp.
#[must_use]
pub fn day_of(time_ns: u64) -> u32 {
    u32::try_from(time_ns / NS_PER_DAY).unwrap_or(u32::MAX)
}

/// Whether a day (days since the Unix epoch) is a Saturday or a Sunday. 1 January 1970 was a
/// Thursday.
#[must_use]
pub fn is_weekend(day: u32) -> bool {
    let weekday_from_monday = (u64::from(day) + 3) % 7;
    weekday_from_monday >= 5
}

/// Volatility-time weight of a day: `weekend_weight` on weekends, `1` otherwise.
#[must_use]
pub fn day_weight(day: u32, weekend_weight: Fixed) -> Fixed {
    if is_weekend(day) {
        weekend_weight
    } else {
        Fixed::ONE
    }
}

#[cfg(test)]
mod tests {
    use super::{day_of, day_weight, is_weekend, NS_PER_DAY};
    use crate::risk::fixed::Fixed;

    #[test]
    fn weekdays_from_epoch() {
        // 1970-01-01 Thursday, 01-03 Saturday, 01-04 Sunday, 01-05 Monday.
        assert!(!is_weekend(0));
        assert!(is_weekend(2));
        assert!(is_weekend(3));
        assert!(!is_weekend(4));
        // 2026-11-07 is a Saturday, 20_764 days after the epoch; 2026-11-09 is a Monday.
        assert!(is_weekend(20_764));
        assert!(!is_weekend(20_766));
    }

    #[test]
    fn day_of_and_weights() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(NS_PER_DAY - 1), 0);
        assert_eq!(day_of(NS_PER_DAY), 1);
        let fx_weekend = Fixed::from_raw(300_000_000_000);
        assert_eq!(day_weight(2, fx_weekend), fx_weekend);
        assert_eq!(day_weight(4, fx_weekend), Fixed::ONE);
    }
}
