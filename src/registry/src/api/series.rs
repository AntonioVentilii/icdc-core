use core::ops::Bound;

use candid::Principal;
use ic_cdk::api::{is_controller, msg_caller, time};
use ic_cdk_macros::{query, update};
use shared::{
    constants::{
        HOUR_NS, MAX_FORKS_PER_SOURCE_PER_USER, MAX_SERIES_DESCRIPTION_LEN,
        MAX_SERIES_RESOLUTION_CLAUSE_LEN, MAX_SERIES_TITLE_LEN,
    },
    types::{
        series::{
            is_valid_locale, AddSeriesParams, AddSeriesResult, ForkSeriesParams, ListSeriesParams,
            PaginationParams, Series, SeriesError, SeriesPage, SeriesStatus,
            UpdateSeriesMetadataParams, UpdateSeriesResult,
        },
        BalanceDomain, EngineRole, NonMonetaryUnit, PayoffType, PayoutUnit, SeriesId,
        SeriesIdParams, TradingAccess,
    },
};

use crate::{
    api::groups::can_principal_see_series,
    guards::{caller_is_not_anonymous, has_engine_role_on},
    memory::{ENGINE_STORE, SERIES_STORE, SOCIAL_CREATION_LOG, SOCIAL_LIMITS},
    utils::canonical_id_part,
};

/// Classification of the caller's authorization tier for series creation.
enum CreationTier {
    /// Full creator (controller or Engine Creator): may create any series.
    Creator,
    /// Social: any authenticated user creating a non-monetary social market, subject to rate
    /// limits.
    Social,
}

fn is_all_restricted(trading_access: &[TradingAccess]) -> bool {
    !trading_access.is_empty()
        && trading_access
            .iter()
            .all(|ta| matches!(ta, TradingAccess::Restricted { .. }))
}

/// Adds a new derivative series to the registry.
///
/// Authorization is tiered:
///
/// 1. **Controllers**: may create any series with or without `engine_id`.
/// 2. **Engine Creators**: must provide `engine_id` referencing an Engine where they hold the
///    `Creator` role.
/// 3. **Any authenticated user**: may create **social** markets (`BalanceDomain::Social` +
///    `NonMonetary` payout) with `Restricted` trading access, subject to per-user rate limits.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn add_series(params: AddSeriesParams) -> AddSeriesResult {
    let caller = msg_caller();
    let caller_is_ctrl = is_controller(&caller);

    let is_social_market = params.balance_domain == BalanceDomain::Social
        && matches!(params.payout_unit, PayoutUnit::NonMonetary(_));

    let tier = if caller_is_ctrl {
        CreationTier::Creator
    } else if let Some(ref eid) = params.engine_id {
        if !has_engine_role_on(&caller, &EngineRole::Creator, eid) {
            return Err(SeriesError::EngineRoleNotHeld).into();
        }
        CreationTier::Creator
    } else if is_social_market {
        CreationTier::Social
    } else {
        return Err(SeriesError::EngineIdRequired).into();
    };

    add_series_impl(params, caller, time(), &tier)
}

/// Inner implementation, separated from the IC update endpoint for testability.
///
/// All IC-dependent values (caller, time, authorization tier) are injected.
fn add_series_impl(
    params: AddSeriesParams,
    caller: Principal,
    now: u64,
    tier: &CreationTier,
) -> AddSeriesResult {
    let AddSeriesParams {
        underlying,
        balance_domain,
        expiry_ns,
        start_ns,
        payoff_type,
        strike,
        settlement_cap,
        price_precision,
        payout_unit,
        oracle_source,
        title,
        description,
        resolution,
        outcomes,
        icon_url,
        banner_url,
        trading_access,
        engine_id,
        locale,
    } = params;

    // --- Social-domain invariants (apply regardless of tier) ---

    let is_social_market = balance_domain == BalanceDomain::Social
        && matches!(payout_unit, PayoutUnit::NonMonetary(_));

    if balance_domain == BalanceDomain::Social {
        if !matches!(payout_unit, PayoutUnit::NonMonetary(_)) {
            return Err(SeriesError::SocialMarketRequiresNonMonetaryPayout).into();
        }
        if !is_all_restricted(&trading_access) {
            return Err(SeriesError::SocialMarketMustBeRestricted).into();
        }
    }

    // --- Tier-specific validation ---

    match &tier {
        CreationTier::Creator => {
            if !is_social_market && payout_unit != PayoutUnit::usd() {
                return Err(SeriesError::UnsupportedPayoutUnit).into();
            }
        }
        CreationTier::Social => {
            if let Err(e) = check_social_rate_limits(&caller, now) {
                return Err(e).into();
            }
        }
    }

    // --- Common validation ---

    if let PayoutUnit::NonMonetary(NonMonetaryUnit::Social(ref reward)) = payout_unit {
        if let Err(e) = reward.validate() {
            return Err(e).into();
        }
    }

    if title.chars().count() > MAX_SERIES_TITLE_LEN {
        return Err(SeriesError::TitleTooLong).into();
    }

    if description.plain.chars().count() > MAX_SERIES_DESCRIPTION_LEN {
        return Err(SeriesError::DescriptionTooLong).into();
    }

    if resolution.clause.trim().is_empty() {
        return Err(SeriesError::ResolutionClauseEmpty).into();
    }

    if resolution.clause.chars().count() > MAX_SERIES_RESOLUTION_CLAUSE_LEN {
        return Err(SeriesError::ResolutionClauseTooLong).into();
    }

    if let Some(ref tag) = locale {
        if !is_valid_locale(tag) {
            return Err(SeriesError::InvalidLocale).into();
        }
    }

    // Linear (forward/NDF/future) invariants: a `settlement_cap` is compulsory
    // and must be strictly positive; the forward rate is the trade price, so a
    // fixed `strike` and categorical `outcomes` are both rejected. Conversely,
    // `settlement_cap` is meaningless on any non-`Linear` payoff.
    if payoff_type == PayoffType::Linear {
        match settlement_cap.as_ref() {
            None => return Err(SeriesError::LinearRequiresSettlementCap).into(),
            Some(cap) if cap.value() == 0 => return Err(SeriesError::InvalidSettlementCap).into(),
            Some(_) => {}
        }
        if outcomes.is_some() {
            return Err(SeriesError::LinearRejectsOutcomes).into();
        }
        if strike.is_some() {
            return Err(SeriesError::LinearRejectsStrike).into();
        }
    } else if settlement_cap.is_some() {
        return Err(SeriesError::SettlementCapOnlyForLinear).into();
    }

    // A scheduled series must have a window it can actually be traded in, and
    // "already live" must have exactly one encoding (`None`). Otherwise the same
    // market could be registered twice under two different ids, since `start_ns`
    // is hashed into the id.
    if let Some(start_ns) = start_ns {
        if start_ns <= now {
            return Err(SeriesError::StartNotInFuture).into();
        }
        if start_ns >= expiry_ns {
            return Err(SeriesError::StartNotBeforeExpiry).into();
        }
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
        start_ns,
        payoff_type: &payoff_type,
        strike: strike.as_ref(),
        settlement_cap: settlement_cap.as_ref(),
        price_precision,
        payout_unit: &payout_unit,
        outcomes: outcomes.as_deref(),
        oracle_source: &oracle_source,
        forked_from: None,
        fork_caller: None,
        fork_index: None,
    });

    let series = Series {
        series_id: series_id.clone(),
        balance_domain,
        underlying,
        expiry_ns,
        start_ns,
        payoff_type,
        strike,
        settlement_cap,
        price_precision,
        payout_unit,
        outcomes,
        oracle_source,
        creator: caller,
        created_at_ns: now,
        title,
        description,
        resolution,
        icon_url,
        banner_url,
        trading_access,
        engine_id,
        forked_from: None,
        locale,
    };

    let res = SERIES_STORE.with(|store| {
        let mut store = store.borrow_mut();

        if store.contains_key(&series_id) {
            return Err(SeriesError::SeriesAlreadyExists);
        }

        store.insert(series_id.clone(), series);

        Ok(series_id)
    });

    if res.is_ok() && matches!(tier, CreationTier::Social) {
        record_social_creation(&caller, now);
    }

    res.into()
}

/// Forks (clones) an existing series into a new restricted-access market.
///
/// The forked series inherits all defining parameters from the source but gets a
/// distinct ID and carries a `forked_from` reference back to the original.
///
/// Authorization is tiered, mirroring [`add_series`]:
///
/// 1. **Controllers**: may fork any series.
/// 2. **Engine Creators**: must provide an `engine_id` on which they hold the `Creator` role.
/// 3. **Any authenticated user**: may fork **social** source markets (`BalanceDomain::Social` +
///    `NonMonetary` payout) into their own closed circle, subject to the same per-user rate limits
///    as social market creation. This is the "Challenge your friends" flow.
///
/// In all cases the fork's `trading_access` must be fully `Restricted`: a fork
/// never widens access.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn fork_series(params: ForkSeriesParams) -> AddSeriesResult {
    let caller = msg_caller();
    let now = time();
    let caller_is_ctrl = is_controller(&caller);

    if !is_all_restricted(&params.trading_access) {
        return Err(SeriesError::ForkMustBeRestricted).into();
    }

    // Authorization tiering: resolve the tier before hitting the store.
    // Rate limits for the Social tier are re-checked inside `fork_series_impl`
    // so that unit tests exercising `_impl` directly still enforce them.
    let tier = if caller_is_ctrl {
        CreationTier::Creator
    } else if let Some(ref eid) = params.engine_id {
        if !has_engine_role_on(&caller, &EngineRole::Creator, eid) {
            return Err(SeriesError::EngineRoleNotHeld).into();
        }
        CreationTier::Creator
    } else {
        // Non-controller, no engine_id: only permitted if the source is a
        // social market.
        let source_is_social = SERIES_STORE.with(|store| {
            store.borrow().get(&params.source_series_id).map(|s| {
                s.balance_domain == BalanceDomain::Social
                    && matches!(s.payout_unit, PayoutUnit::NonMonetary(_))
            })
        });

        match source_is_social {
            Some(true) => CreationTier::Social,
            Some(false) => return Err(SeriesError::EngineIdRequired).into(),
            None => return Err(SeriesError::SourceSeriesNotFound).into(),
        }
    };

    fork_series_impl(params, caller, now, &tier)
}

fn fork_series_impl(
    params: ForkSeriesParams,
    caller: Principal,
    now: u64,
    tier: &CreationTier,
) -> AddSeriesResult {
    if matches!(tier, CreationTier::Social) {
        if let Err(e) = check_social_rate_limits(&caller, now) {
            return Err(e).into();
        }
    }

    let res: Result<SeriesId, SeriesError> = SERIES_STORE.with(|store| {
        let mut store = store.borrow_mut();

        let source = store
            .get(&params.source_series_id)
            .cloned()
            .ok_or(SeriesError::SourceSeriesNotFound)?;

        let title = params.title.unwrap_or_else(|| source.title.clone());
        let description = params
            .description
            .unwrap_or_else(|| source.description.clone());
        let resolution = params
            .resolution
            .unwrap_or_else(|| source.resolution.clone());
        let locale = params.locale.or_else(|| source.locale.clone());

        if title.chars().count() > MAX_SERIES_TITLE_LEN {
            return Err(SeriesError::TitleTooLong);
        }
        if description.plain.chars().count() > MAX_SERIES_DESCRIPTION_LEN {
            return Err(SeriesError::DescriptionTooLong);
        }
        if resolution.clause.trim().is_empty() {
            return Err(SeriesError::ResolutionClauseEmpty);
        }
        if resolution.clause.chars().count() > MAX_SERIES_RESOLUTION_CLAUSE_LEN {
            return Err(SeriesError::ResolutionClauseTooLong);
        }
        if let Some(ref tag) = locale {
            if !is_valid_locale(tag) {
                return Err(SeriesError::InvalidLocale);
            }
        }

        let fork_index = store
            .values()
            .filter(|s| s.forked_from.as_ref() == Some(&source.series_id) && s.creator == caller)
            .count() as u64;

        if fork_index >= MAX_FORKS_PER_SOURCE_PER_USER {
            return Err(SeriesError::ForkLimitReached);
        }

        let series_id = Series::generate_id(&SeriesIdParams {
            underlying: &source.underlying,
            balance_domain: source.balance_domain,
            expiry_ns: source.expiry_ns,
            // A fork is the source contract re-titled, so it inherits the whole
            // trading window verbatim, including a start that has already passed,
            // which simply means the fork is open immediately. Forks cannot
            // collide regardless (`fork_caller` + `fork_index` make the id
            // unique), so the "future start only" rule that `add_series` enforces
            // to keep ids canonical has nothing to protect here.
            start_ns: source.start_ns,
            payoff_type: &source.payoff_type,
            strike: source.strike.as_ref(),
            settlement_cap: source.settlement_cap.as_ref(),
            price_precision: source.price_precision,
            payout_unit: &source.payout_unit,
            outcomes: source.outcomes.as_deref(),
            oracle_source: &source.oracle_source,
            forked_from: Some(&source.series_id),
            fork_caller: Some(&caller),
            fork_index: Some(fork_index),
        });

        if store.contains_key(&series_id) {
            return Err(SeriesError::SeriesAlreadyExists);
        }

        let series = Series {
            series_id: series_id.clone(),
            underlying: source.underlying,
            balance_domain: source.balance_domain,
            expiry_ns: source.expiry_ns,
            start_ns: source.start_ns,
            payoff_type: source.payoff_type,
            strike: source.strike,
            settlement_cap: source.settlement_cap,
            price_precision: source.price_precision,
            payout_unit: source.payout_unit,
            outcomes: source.outcomes,
            oracle_source: source.oracle_source,
            creator: caller,
            created_at_ns: now,
            title,
            description,
            resolution,
            icon_url: source.icon_url,
            banner_url: source.banner_url,
            trading_access: params.trading_access,
            engine_id: params.engine_id,
            forked_from: Some(source.series_id),
            locale,
        };

        store.insert(series_id.clone(), series);
        Ok(series_id)
    });

    if res.is_ok() && matches!(tier, CreationTier::Social) {
        record_social_creation(&caller, now);
    }

    res.into()
}

/// Validates that the caller has not exceeded social market rate limits.
fn check_social_rate_limits(caller: &Principal, now: u64) -> Result<(), SeriesError> {
    let limits = SOCIAL_LIMITS.with(|l| l.borrow().clone());

    SOCIAL_CREATION_LOG.with(|log| {
        let log = log.borrow();
        let timestamps = log.get(caller);

        let total = timestamps.map_or(0, Vec::len) as u64;
        if total >= limits.max_per_user {
            return Err(SeriesError::SocialMaxPerUserReached);
        }

        let cutoff = now.saturating_sub(HOUR_NS);
        let recent = timestamps.map_or(0, |ts| ts.iter().filter(|&&t| t >= cutoff).count()) as u64;
        if recent >= limits.max_per_hour {
            return Err(SeriesError::SocialRateLimitExceeded);
        }

        Ok(())
    })
}

/// Records a successful social market creation for rate-limit tracking.
fn record_social_creation(caller: &Principal, now: u64) {
    SOCIAL_CREATION_LOG.with(|log| {
        let mut log = log.borrow_mut();
        log.entry(*caller).or_default().push(now);
    });
}

/// Returns `true` if `caller` may edit metadata on `series`, given whether the
/// caller is a canister controller.
///
/// Authorized callers are: canister controllers, the series' original
/// `creator`, and the admins of the series' owning Engine (if any). The
/// controller status is passed in (rather than read via `is_controller` here)
/// so this predicate stays a pure function of the store and is unit-testable.
fn caller_can_manage_series_metadata(
    series: &Series,
    caller: &Principal,
    caller_is_controller: bool,
) -> bool {
    if caller_is_controller || series.creator == *caller {
        return true;
    }

    series.engine_id.as_ref().is_some_and(|eid| {
        ENGINE_STORE.with(|store| {
            store
                .borrow()
                .get(eid)
                .is_some_and(|engine| engine.admins.contains(caller))
        })
    })
}

/// Updates **non-critical** metadata on an existing series.
///
/// Only `description`, `icon_url`, `banner_url`, and `locale` can be changed.
/// The series' identity-bearing economic fields, `title`, and `resolution` are
/// immutable here by design (see [`UpdateSeriesMetadataParams`]).
///
/// # Authorization
///
/// Controllers, the series' `creator`, and admins of the series' Engine may
/// update metadata. All other callers receive [`SeriesError::Unauthorized`].
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn update_series_metadata(params: UpdateSeriesMetadataParams) -> UpdateSeriesResult {
    let caller = msg_caller();
    update_series_metadata_impl(params, caller, is_controller(&caller)).into()
}

/// Implementation of [`update_series_metadata`] with an injectable caller and
/// controller flag for unit tests.
fn update_series_metadata_impl(
    params: UpdateSeriesMetadataParams,
    caller: Principal,
    caller_is_controller: bool,
) -> Result<Series, SeriesError> {
    // Validate inputs before taking the store borrow.
    if let Some(ref description) = params.description {
        if description.plain.chars().count() > MAX_SERIES_DESCRIPTION_LEN {
            return Err(SeriesError::DescriptionTooLong);
        }
    }

    if let Some(Some(ref tag)) = params.locale {
        if !is_valid_locale(tag) {
            return Err(SeriesError::InvalidLocale);
        }
    }

    SERIES_STORE.with(|store| {
        let mut store = store.borrow_mut();
        let series = store
            .get_mut(&params.series_id)
            .ok_or(SeriesError::SeriesNotFound)?;

        if !caller_can_manage_series_metadata(series, &caller, caller_is_controller) {
            return Err(SeriesError::Unauthorized);
        }

        if let Some(description) = params.description {
            series.description = description;
        }
        if let Some(icon_url) = params.icon_url {
            series.icon_url = icon_url;
        }
        if let Some(banner_url) = params.banner_url {
            series.banner_url = banner_url;
        }
        if let Some(locale) = params.locale {
            series.locale = locale;
        }

        Ok(series.clone())
    })
}

/// Retrieves a specific [`Series`] by its [`SeriesId`].
#[query]
#[must_use]
pub fn get_series(series_id: SeriesId) -> Option<Series> {
    SERIES_STORE.with(move |store| store.borrow().get(&series_id).cloned())
}

/// Returns where a series currently sits in its trading window, or `None` if the
/// series is not registered.
///
/// Clients could derive this from `start_ns`/`expiry_ns` themselves, but only
/// against their own clock. This evaluates the window with the canister's
/// `time()`, which is the same clock the trading gate uses, so a countdown built
/// on it cannot disagree with whether an order will actually be accepted.
///
/// Settlement state is not folded in: it lives in the clearing canister. A series
/// reported [`SeriesStatus::Expired`] here may or may not be settled yet.
#[query]
#[must_use]
pub fn get_series_status(series_id: SeriesId) -> Option<SeriesStatus> {
    let now = time();
    SERIES_STORE.with(move |store| store.borrow().get(&series_id).map(|s| s.status(now)))
}

/// Returns a paginated page of registered derivative series, optionally filtered.
///
/// # Visibility
///
/// Results are scoped to what the caller is allowed to see. Restricted series
/// are omitted unless the caller is a controller, the series creator, or a
/// member of at least one group referenced by the series' `trading_access`.
/// See [`can_principal_see_series`] for the full predicate.
#[query]
#[must_use]
pub fn list_series_with(params: ListSeriesParams) -> SeriesPage {
    list_series_with_impl(params, msg_caller(), time())
}

/// Returns a paginated page of all registered derivative series visible to the caller.
#[query]
#[must_use]
pub fn list_series(pagination: PaginationParams) -> SeriesPage {
    let params = ListSeriesParams {
        pagination: Some(pagination),
        ..Default::default()
    };

    list_series_with_impl(params, msg_caller(), time())
}

/// Implementation of `list_series_with` with an injectable caller and clock for
/// unit tests.
///
/// `now` is the cutoff for every clock-dependent filter (`only_unexpired` and
/// `tradeable_now`), applied together via [`ListSeriesParams::matches_time`].
/// The public query passes the canister's `time()`; tests pass an explicit
/// value.
#[must_use]
fn list_series_with_impl(params: ListSeriesParams, caller: Principal, now: u64) -> SeriesPage {
    SERIES_STORE.with(move |store| {
        let store = store.borrow();

        let cursor = params.pagination.as_ref().and_then(|p| p.cursor.as_ref());

        let range = match cursor {
            Some(c) => store.range((Bound::Excluded(c), Bound::Unbounded)),
            None => store.range(..),
        };

        let iter = range
            .filter(|(_, s)| params.matches(s))
            .filter(|(_, s)| params.matches_time(s, now))
            .filter(|(_, s)| can_principal_see_series(s, &caller));

        let (items, next_cursor) = PaginationParams::apply(params.pagination.as_ref(), iter);

        SeriesPage { items, next_cursor }
    })
}

#[cfg(test)]
mod tests;
