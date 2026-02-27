use ic_cdk_macros::{query, update};
use shared::{
    constants::{MAX_SERIES_DESCRIPTION_LEN, MAX_SERIES_TITLE_LEN},
    types::{Series, SeriesId},
};

use crate::{
    errors::RegistryError, memory::SERIES_STORE, params::AddSeriesParams, results::AddSeriesResult,
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
            title,
            description,
        } = params;

        if title.chars().count() > MAX_SERIES_TITLE_LEN {
            return Err(RegistryError::TitleTooLong).into();
        }

        if description.chars().count() > MAX_SERIES_DESCRIPTION_LEN {
            return Err(RegistryError::DescriptionTooLong).into();
        }

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
            creator: ic_cdk::caller(),
            title,
            description,
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
