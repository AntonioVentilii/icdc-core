use core::ops::Bound;

use ic_cdk::api::{msg_caller, time};
use ic_cdk_macros::{query, update};
use shared::{
    constants::{MAX_SERIES_DESCRIPTION_LEN, MAX_SERIES_TITLE_LEN},
    types::{
        series::{
            AddSeriesParams, AddSeriesResult, ListSeriesParams, PaginationParams, Series,
            SeriesError, SeriesPage,
        },
        PayoutUnit, SeriesId, SeriesIdParams, TradingAccess,
    },
};

use crate::{guards::caller_is_authorized_creator, memory::SERIES_STORE, utils::canonical_id_part};

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
/// * [`AddSeriesResult::Err`] with [`SeriesError::SeriesAlreadyExists`] if the series is already
///   registered.
#[update(guard = "caller_is_authorized_creator")]
#[must_use]
pub fn add_series(params: AddSeriesParams) -> AddSeriesResult {
    let AddSeriesParams {
        underlying,
        balance_domain,
        expiry_ns,
        payoff_type,
        strike,
        price_precision,
        payout_unit,
        oracle_source,
        title,
        description,
        outcomes,
        icon_url,
        banner_url,
        trading_access,
    } = params;

    // Validate payout unit - currently only USD is supported across the protocol
    if payout_unit != PayoutUnit::usd() {
        return Err(SeriesError::UnsupportedPayoutUnit).into();
    }

    if title.chars().count() > MAX_SERIES_TITLE_LEN {
        return Err(SeriesError::TitleTooLong).into();
    }

    if description.plain.chars().count() > MAX_SERIES_DESCRIPTION_LEN {
        return Err(SeriesError::DescriptionTooLong).into();
    }

    let trading_access = if trading_access.is_empty() {
        vec![TradingAccess::Open]
    } else {
        trading_access
    };

    let underlying = canonical_id_part(&underlying);
    let oracle_source = canonical_id_part(&oracle_source);

    let series_id = Series::generate_id(&SeriesIdParams {
        underlying: &underlying,
        balance_domain,
        expiry_ns,
        payoff_type: &payoff_type,
        strike: strike.as_ref(),
        price_precision,
        payout_unit: &payout_unit,
        outcomes: outcomes.as_deref(),
        oracle_source: &oracle_source,
    });

    let series = Series {
        series_id: series_id.clone(),
        balance_domain,
        underlying,
        expiry_ns,
        payoff_type,
        strike,
        price_precision,
        payout_unit,
        outcomes,
        oracle_source,
        creator: msg_caller(),
        created_at_ns: time(),
        title,
        description,
        icon_url,
        banner_url,
        trading_access,
    };

    let res = SERIES_STORE.with(|store| {
        let mut store = store.borrow_mut();

        if store.contains_key(&series_id) {
            return Err(SeriesError::SeriesAlreadyExists);
        }

        store.insert(series_id.clone(), series);

        Ok(series_id)
    });

    res.into()
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
#[must_use]
pub fn get_series(series_id: SeriesId) -> Option<Series> {
    SERIES_STORE.with(move |store| store.borrow().get(&series_id).cloned())
}

/// Returns a paginated page of registered derivative series, optionally filtered.
#[query]
#[must_use]
pub fn list_series_with(params: ListSeriesParams) -> SeriesPage {
    SERIES_STORE.with(move |store| {
        let store = store.borrow();

        let cursor = params.pagination.as_ref().and_then(|p| p.cursor.as_ref());

        // Optimization: use range to skip directly to the cursor (O(log N)).
        // Even with filters, starting FROM the cursor is always more efficient.
        let range = match cursor {
            Some(c) => store.range((Bound::Excluded(c), Bound::Unbounded)),
            None => store.range(..),
        };

        let iter = range.filter(|(_, s)| params.matches(s));

        // PaginationParams::apply handles the limit and next_cursor.
        // Since 'range' already handled the cursor skip, 'apply' just needs to take(limit + 1).
        let (items, next_cursor) = PaginationParams::apply(params.pagination.as_ref(), iter);

        SeriesPage { items, next_cursor }
    })
}

/// Returns a paginated page of all registered derivative series.
#[query]
#[must_use]
pub fn list_series(pagination: PaginationParams) -> SeriesPage {
    let params = ListSeriesParams {
        pagination: Some(pagination),
        ..Default::default()
    };

    list_series_with(params)
}

#[cfg(test)]
mod tests {
    use shared::types::{BalanceDomain, Description, FiatUnit, PayoffType, PayoutUnit};

    use crate::{api::add_series, AddSeriesParams, AddSeriesResult, SeriesError};

    #[test]
    fn add_series_unsupported_payout_unit() {
        let params = AddSeriesParams {
            underlying: "ICP".to_owned(),
            balance_domain: BalanceDomain::Settlement,
            expiry_ns: 1000,
            payoff_type: PayoffType::Call,
            strike: None,
            price_precision: 8,
            payout_unit: PayoutUnit::Fiat(FiatUnit::Eur), // Unsupported
            oracle_source: "oracle".to_owned(),
            title: "Test".to_owned(),
            description: Description::plain("Test"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            trading_access: vec![],
        };

        let result = add_series(params);
        assert!(matches!(
            result,
            AddSeriesResult::Err(SeriesError::UnsupportedPayoutUnit)
        ));
    }
}
