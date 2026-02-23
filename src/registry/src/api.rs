use ic_cdk_macros::{query, update};
use shared::types::{Series, SeriesId};

use crate::{
    errors::RegistryError, memory::SERIES_STORE, params::AddSeriesParams, results::AddSeriesResult,
    utils::canonical_id_part,
};

#[update]
pub fn add_series(params: AddSeriesParams) -> AddSeriesResult {
    let result: Result<SeriesId, RegistryError> = {
        let AddSeriesParams {
            underlying,
            expiry,
            payoff_type,
            strike,
            settlement_asset,
            oracle_source,
        } = params;

        let underlying = canonical_id_part(&underlying);
        let oracle_source = canonical_id_part(&oracle_source);

        let series_id = Series::generate_id(
            &underlying,
            expiry,
            &payoff_type,
            strike,
            &settlement_asset,
            &oracle_source,
        );

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
            let mut store = store.borrow_mut();

            if store.contains_key(&series_id) {
                return Err(RegistryError::SeriesAlreadyExists);
            }

            store.insert(series_id.clone(), series);

            Ok(series_id)
        })
    };

    result.into()
}

#[query]
pub fn get_series(series_id: SeriesId) -> Option<Series> {
    SERIES_STORE.with(|store| store.borrow().get(&series_id).cloned())
}

#[query]
pub fn list_series() -> Vec<Series> {
    SERIES_STORE.with(|store| store.borrow().values().cloned().collect())
}
