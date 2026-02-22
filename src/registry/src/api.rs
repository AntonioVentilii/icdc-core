use ic_cdk_macros::{query, update};
use shared::Series;

use crate::{memory::SERIES_STORE, params::AddSeriesParams, utils::canonical_id_part};

#[update]
pub fn add_series(params: AddSeriesParams) -> String {
    let AddSeriesParams {
        underlying,
        expiry,
        payoff_type,
        strike,
        settlement_asset,
        oracle_source,
    } = params;

    let underlying = canonical_id_part(&underlying);
    let settlement_asset = canonical_id_part(&settlement_asset);
    let oracle_source = canonical_id_part(&oracle_source);

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
