//! Daily price history per underlying and the timelocked backfill (`docs/risk/margin-model.md`,
//! section 9.2).

use candid::{CandidType, Principal};
use hex::encode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::risk::fixed::Fixed;

/// Maximum number of points in one chunk.
pub const MAX_POINTS_PER_CHUNK: usize = 2_000;

/// Maximum number of points in one upload (about 55 years of daily data).
pub const MAX_POINTS_PER_UPLOAD: usize = 20_000;

/// Maximum number of uploads that are open or sealed at the same time.
pub const MAX_PENDING_UPLOADS: usize = 8;

/// Maximum length of the `source` and `series_id` provenance fields.
pub const MAX_PROVENANCE_LEN: usize = 64;

/// One daily observation: `price` in USD per unit of the underlying, on `date` (days since the Unix
/// epoch, UTC).
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PricePoint {
    pub date: u32,
    pub price: Fixed,
}

/// A daily price history, sorted by date, one point per date.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PriceHistory {
    pub points: Vec<PricePoint>,
}

impl PriceHistory {
    /// Merges points into the history: a point replaces the existing point on the same date, new
    /// dates are inserted.
    ///
    /// Runs in `O((n + m) log(n + m))`: the incoming points are appended, the whole history is
    /// stably sorted by date, and each run of equal dates collapses onto its last element, which is
    /// the incoming point because the stable sort keeps existing points first.
    pub fn merge(&mut self, incoming: &[PricePoint]) {
        self.points.extend_from_slice(incoming);
        self.points.sort_by_key(|p| p.date);
        self.points.dedup_by(|later, earlier| {
            if later.date == earlier.date {
                *earlier = *later;
                true
            } else {
                false
            }
        });
    }

    /// The points dated `from_date..=to_date`.
    #[must_use]
    pub fn range(&self, from_date: u32, to_date: u32) -> &[PricePoint] {
        let start = self.points.partition_point(|p| p.date < from_date);
        let end = self.points.partition_point(|p| p.date <= to_date);
        &self.points[start..end.max(start)]
    }

    /// Removes the points dated `from_date..=to_date`. Returns how many were removed.
    pub fn remove_range(&mut self, from_date: u32, to_date: u32) -> usize {
        let before = self.points.len();
        self.points
            .retain(|p| p.date < from_date || p.date > to_date);
        before - self.points.len()
    }

    /// Appends today's point if the history has none for that date yet. Returns whether it
    /// appended.
    pub fn append_if_new(&mut self, point: PricePoint) -> bool {
        if self
            .points
            .last()
            .is_some_and(|last| last.date >= point.date)
        {
            return false;
        }
        self.points.push(point);
        true
    }
}

/// Lifecycle of a backfill upload.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UploadStatus {
    /// Accepting chunks.
    Open,
    /// Hash verified; activation proposed through the timelock as pending action `action_id`.
    Sealed { action_id: u64 },
    /// Merged into the history.
    Activated { activated_at_ns: u64 },
    /// Its activation was cancelled; it will never be merged.
    Cancelled,
}

/// A backfill upload with its provenance.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HistoryUpload {
    pub id: u64,
    pub underlying: String,
    pub source: String,
    pub series_id: String,
    /// Points waiting to be merged. Cleared once the upload is activated or cancelled.
    pub points: Vec<PricePoint>,
    pub point_count: u32,
    pub first_date: Option<u32>,
    pub last_date: Option<u32>,
    /// Lower-case hex SHA-256 of the canonical encoding, recorded when the upload is sealed.
    pub sha256: Option<String>,
    pub status: UploadStatus,
    pub created_by: Principal,
    pub created_at_ns: u64,
}

/// Why a chunk was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkError {
    Empty,
    TooLarge,
    UploadFull,
    NotIncreasing { date: u32 },
    NonPositivePrice { date: u32 },
    FutureDate { date: u32 },
}

impl HistoryUpload {
    /// Validates and appends a chunk: dates strictly increasing across chunks and not after
    /// `today`, prices strictly positive. A future date would otherwise sit in the history and
    /// block every later daily point.
    pub fn append(&mut self, chunk: &[PricePoint], today: u32) -> Result<(), ChunkError> {
        if chunk.is_empty() {
            return Err(ChunkError::Empty);
        }
        if chunk.len() > MAX_POINTS_PER_CHUNK {
            return Err(ChunkError::TooLarge);
        }
        if self.points.len() + chunk.len() > MAX_POINTS_PER_UPLOAD {
            return Err(ChunkError::UploadFull);
        }
        let mut previous = self.points.last().map(|p| p.date);
        for point in chunk {
            if point.price <= Fixed::ZERO {
                return Err(ChunkError::NonPositivePrice { date: point.date });
            }
            if point.date > today {
                return Err(ChunkError::FutureDate { date: point.date });
            }
            if previous.is_some_and(|date| point.date <= date) {
                return Err(ChunkError::NotIncreasing { date: point.date });
            }
            previous = Some(point.date);
        }
        self.points.extend_from_slice(chunk);
        self.point_count = u32::try_from(self.points.len()).unwrap_or(u32::MAX);
        self.first_date = self.points.first().map(|p| p.date);
        self.last_date = self.points.last().map(|p| p.date);
        Ok(())
    }
}

/// SHA-256 of the canonical encoding, as lower-case hex.
///
/// For each point in ascending date order: the date as a 4-byte big-endian `u32`, then the price as
/// a 16-byte big-endian `i128` at 12 decimals. Points are concatenated with no separator and no
/// header, so an off-chain script can recompute the hash from the public source before the upload
/// is activated.
#[must_use]
pub fn canonical_sha256(points: &[PricePoint]) -> String {
    let mut hasher = Sha256::new();
    for point in points {
        hasher.update(point.date.to_be_bytes());
        hasher.update(point.price.raw().to_be_bytes());
    }
    encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use candid::Principal;

    use super::{
        canonical_sha256, ChunkError, HistoryUpload, PriceHistory, PricePoint, UploadStatus,
    };
    use crate::risk::fixed::Fixed;

    fn point(date: u32, price: i64) -> PricePoint {
        PricePoint {
            date,
            price: Fixed::from_int(price),
        }
    }

    fn open_upload() -> HistoryUpload {
        HistoryUpload {
            id: 0,
            underlying: "BTC".to_owned(),
            source: "coinbase".to_owned(),
            series_id: "BTC-USD".to_owned(),
            points: vec![],
            point_count: 0,
            first_date: None,
            last_date: None,
            sha256: None,
            status: UploadStatus::Open,
            created_by: Principal::anonymous(),
            created_at_ns: 0,
        }
    }

    const TODAY: u32 = 100;

    #[test]
    fn chunks_must_increase_and_be_positive() {
        let mut upload = open_upload();
        upload
            .append(&[point(1, 100), point(2, 101)], TODAY)
            .unwrap();
        upload.append(&[point(3, 102)], TODAY).unwrap();
        assert_eq!(upload.point_count, 3);
        assert_eq!(upload.first_date, Some(1));
        assert_eq!(upload.last_date, Some(3));

        assert_eq!(
            upload.append(&[point(3, 103)], TODAY),
            Err(ChunkError::NotIncreasing { date: 3 })
        );
        assert_eq!(
            upload.append(&[point(4, 0)], TODAY),
            Err(ChunkError::NonPositivePrice { date: 4 })
        );
        assert_eq!(upload.append(&[], TODAY), Err(ChunkError::Empty));
        // A rejected chunk leaves the upload untouched.
        assert_eq!(upload.point_count, 3);
    }

    #[test]
    fn chunks_cannot_carry_future_dates() {
        let mut upload = open_upload();
        upload.append(&[point(TODAY, 100)], TODAY).unwrap();
        assert_eq!(
            upload.append(&[point(TODAY + 1, 100)], TODAY),
            Err(ChunkError::FutureDate { date: TODAY + 1 })
        );
        assert_eq!(upload.point_count, 1);
    }

    #[test]
    fn merge_replaces_dates_and_inserts_new_ones() {
        let mut history = PriceHistory {
            points: vec![point(1, 100), point(3, 103)],
        };
        history.merge(&[point(2, 102), point(3, 999), point(4, 104)]);
        assert_eq!(
            history.points,
            vec![point(1, 100), point(2, 102), point(3, 999), point(4, 104)]
        );
    }

    #[test]
    fn large_merges_of_older_and_overlapping_data_stay_sorted() {
        // 20,000 recent points, then 20,000 older points overlapping the first 1,000 recent dates:
        // the case that made the insertion-based merge quadratic.
        let recent: Vec<PricePoint> = (19_000..39_000).map(|d| point(d, 100)).collect();
        let older: Vec<PricePoint> = (0..20_000).map(|d| point(d, 200)).collect();
        let mut history = PriceHistory::default();
        history.merge(&recent);
        history.merge(&older);

        assert_eq!(history.points.len(), 39_000);
        assert!(history.points.windows(2).all(|w| w[0].date < w[1].date));
        // Overlapping dates take the incoming (older upload's) value; the rest keep theirs.
        assert_eq!(history.points[19_500], point(19_500, 200));
        assert_eq!(history.points[25_000], point(25_000, 100));
    }

    #[test]
    fn ranges_select_and_remove_inclusively() {
        let mut history = PriceHistory {
            points: (1..=10).map(|d| point(d, 100)).collect(),
        };
        assert_eq!(
            history.range(3, 5),
            &[point(3, 100), point(4, 100), point(5, 100)]
        );
        assert!(history.range(11, 20).is_empty());
        assert!(history.range(5, 3).is_empty());
        assert_eq!(history.remove_range(3, 5), 3);
        assert_eq!(history.points.len(), 7);
        assert!(history.range(3, 5).is_empty());
    }

    #[test]
    fn append_if_new_skips_existing_dates() {
        let mut history = PriceHistory {
            points: vec![point(5, 100)],
        };
        assert!(!history.append_if_new(point(5, 101)));
        assert!(!history.append_if_new(point(4, 101)));
        assert!(history.append_if_new(point(6, 101)));
        assert_eq!(history.points.len(), 2);
    }

    #[test]
    fn canonical_hash_is_order_and_value_sensitive() {
        let a = [point(1, 100), point(2, 101)];
        let b = [point(1, 100), point(2, 102)];
        assert_eq!(canonical_sha256(&a), canonical_sha256(&a));
        assert_ne!(canonical_sha256(&a), canonical_sha256(&b));
        assert_eq!(canonical_sha256(&a).len(), 64);
        // Empty input hashes to the SHA-256 of the empty string.
        assert_eq!(
            canonical_sha256(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
