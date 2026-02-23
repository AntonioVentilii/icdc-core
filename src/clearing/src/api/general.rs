use ic_cdk_macros::query;
use shared::types::Series;

use crate::memory::SERIES;

#[query]
pub fn list_series() -> Vec<Series> {
    SERIES.with(|s| s.borrow().values().cloned().collect())
}
