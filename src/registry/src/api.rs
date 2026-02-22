use ic_cdk_macros::{query, update};
use shared::{PayoffType, Series};

use crate::memory::SERIES_STORE;

#[update]
pub fn add_series(
    underlying: String,
    expiry: u64,
    payoff_type: PayoffType,
    strike: Option<u64>,
    settlement_asset: String,
    oracle_source: String,
) -> String {
    let series_id = Series::generate_id(&underlying, expiry, &payoff_type, strike);

    let series = Series {
        series_id: series_id.clone(),
        underlying,
        expiry,
        payoff_type,
        strike,
        settlement_asset,
        oracle_source,
    };

    SERIES_STORE.with(|store| {
        store.borrow_mut().insert(series_id.clone(), series);
    });

    series_id
}

#[query]
pub fn get_series(series_id: String) -> Option<Series> {
    SERIES_STORE.with(|store| store.borrow().get(&series_id).cloned())
}

#[query]
pub fn list_series() -> Vec<Series> {
    SERIES_STORE.with(|store| store.borrow().values().cloned().collect())
}
