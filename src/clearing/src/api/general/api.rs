use ic_cdk_macros::query;
use shared::{constants::USD_DECIMALS, types::Series};

use crate::{guards::caller_is_not_anonymous, memory::SERIES};

/// Returns a list of all derivative series currently cached in the clearing canister.
#[query]
#[must_use]
pub fn list_series() -> Vec<Series> {
    SERIES.with(|s| s.borrow().values().cloned().collect())
}

/// Returns the official number of decimals for USD accounting.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn get_usd_decimals() -> u8 {
    USD_DECIMALS
}
