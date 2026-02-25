use ic_cdk_macros::{query, update};
use shared::types::{Series, SeriesId};

use crate::{
    errors::RegistryError,
    memory::{get_or_create_underlying_id, SERIES_STORE, UNDERLYING_IDS},
    params::AddSeriesParams,
    results::AddSeriesResult,
    utils::canonical_id_part,
};

/// Adds a new derivative series to the registry.
///
/// This method generates a canonical [`SeriesId`] for the provided parameters.
/// If the series already exists, it returns an error.
///
/// # Arguments
/// * `params` - The defining parameters for the new series.
///
/// # Returns
/// * [`AddSeriesResult::Ok`] containing the new [`SeriesId`] on success.
/// * [`AddSeriesResult::Err`] with [`RegistryError::SeriesAlreadyExists`] if the series is already
///   registered.
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

        let underlying_ticker = canonical_id_part(&underlying);
        let oracle_source = canonical_id_part(&oracle_source);
        let underlying_id = get_or_create_underlying_id(&underlying_ticker);

        let series_id = Series::generate_id(
            &underlying_ticker,
            underlying_id,
            expiry,
            &payoff_type,
            strike,
            &settlement_asset,
            &oracle_source,
        );

        let series = Series {
            series_id: series_id.clone(),
            underlying: underlying_ticker,
            underlying_id,
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

/// Retrieves a specific [`Series`] by its [`SeriesId`].
///
/// # Arguments
/// * `series_id` - The unique identifier of the series to retrieve.
///
/// # Returns
/// * `Some(Series)` if the series exists in the registry.
/// * `None` otherwise.
#[query]
pub fn get_series(series_id: SeriesId) -> Option<Series> {
    SERIES_STORE.with(|store| store.borrow().get(&series_id).cloned())
}

/// Returns a list of all registered derivative series.
#[query]
pub fn list_series() -> Vec<Series> {
    SERIES_STORE.with(|store| store.borrow().values().cloned().collect())
}

/// Retrieves the canonical ID for a given underlying asset ticker.
///
/// # Arguments
/// * `ticker` - The asset ticker (e.g., "BTC/USD").
///
/// # Returns
/// * `Some(u32)` containing the canonical ID if found.
/// * `None` otherwise.
#[query]
pub fn get_underlying_id(ticker: String) -> Option<u32> {
    UNDERLYING_IDS.with(|ids| ids.borrow().get(&ticker.to_uppercase()).cloned())
}
