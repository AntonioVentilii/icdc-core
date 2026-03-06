use ic_cdk_macros::query;
use shared::types::Series;

use crate::memory::SERIES;

/// Returns a list of all derivative series currently cached in the clearing canister.
#[query]
pub fn list_series() -> Vec<Series> {
    SERIES.with(|s| s.borrow().values().cloned().collect())
}
