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
            PaginationParams, Series, SeriesError, SeriesPage, UpdateSeriesMetadataParams,
            UpdateSeriesResult,
        },
        BalanceDomain, EngineRole, NonMonetaryUnit, PayoutUnit, SeriesId, SeriesIdParams,
        TradingAccess,
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
        payoff_type,
        strike,
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
        forked_from: None,
        fork_caller: None,
        fork_index: None,
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
/// In all cases the fork's `trading_access` must be fully `Restricted` — a fork
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
            payoff_type: &source.payoff_type,
            strike: source.strike.as_ref(),
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
            payoff_type: source.payoff_type,
            strike: source.strike,
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
/// immutable here by design — see [`UpdateSeriesMetadataParams`].
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
/// `now` is the cutoff used by the `only_unexpired` filter (see
/// [`ListSeriesParams::matches_expiry`]). The public query passes the canister's
/// `time()`; tests pass an explicit value.
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
            .filter(|(_, s)| params.matches_expiry(s, now))
            .filter(|(_, s)| can_principal_see_series(s, &caller));

        let (items, next_cursor) = PaginationParams::apply(params.pagination.as_ref(), iter);

        SeriesPage { items, next_cursor }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use candid::Principal;
    use shared::{
        constants::{HOUR_NS, MAX_FORKS_PER_SOURCE_PER_USER, MAX_SERIES_DESCRIPTION_LEN},
        types::{
            engine::{Engine, EngineId},
            groups::GroupId,
            BalanceDomain, Description, FiatUnit, Group, NonMonetaryUnit, PayoffType, PayoutUnit,
            Resolution, SocialLimits, SocialReward, TradingAccess,
        },
    };

    use super::{
        add_series_impl, fork_series_impl, list_series_with_impl, update_series_metadata_impl,
        CreationTier,
    };
    use crate::{
        memory::{ENGINE_STORE, GROUPS_STORE, SERIES_STORE, SOCIAL_CREATION_LOG, SOCIAL_LIMITS},
        AddSeriesParams, AddSeriesResult, ForkSeriesParams, ListSeriesParams, PaginationParams,
        Series, SeriesError, SeriesId, UpdateSeriesMetadataParams,
    };

    fn test_principal(id: u8) -> Principal {
        Principal::from_slice(&[id])
    }

    fn add_as_creator(params: AddSeriesParams, caller: Principal, now: u64) -> AddSeriesResult {
        add_series_impl(params, caller, now, &CreationTier::Creator)
    }

    fn add_as_social(params: AddSeriesParams, caller: Principal, now: u64) -> AddSeriesResult {
        add_series_impl(params, caller, now, &CreationTier::Social)
    }

    fn fork_as_creator(params: ForkSeriesParams, caller: Principal, now: u64) -> AddSeriesResult {
        fork_series_impl(params, caller, now, &CreationTier::Creator)
    }

    fn fork_as_social(params: ForkSeriesParams, caller: Principal, now: u64) -> AddSeriesResult {
        fork_series_impl(params, caller, now, &CreationTier::Social)
    }

    fn base_params() -> AddSeriesParams {
        AddSeriesParams {
            underlying: "ICP".to_owned(),
            balance_domain: BalanceDomain::Settlement,
            expiry_ns: 1000,
            payoff_type: PayoffType::Binary,
            strike: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_owned(),
            title: "Test".to_owned(),
            description: Description::plain("Test"),
            resolution: Resolution::new("Settles per oracle at expiry"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            trading_access: vec![],
            engine_id: None,
            locale: None,
        }
    }

    fn social_params(expiry_ns: u64) -> AddSeriesParams {
        let group = GroupId::from("grp_0".to_owned());
        AddSeriesParams {
            underlying: "PIZZA_CHALLENGE".to_owned(),
            balance_domain: BalanceDomain::Social,
            expiry_ns,
            payoff_type: PayoffType::Binary,
            strike: None,
            price_precision: 0,
            payout_unit: PayoutUnit::NonMonetary(NonMonetaryUnit::Social(SocialReward {
                title: "Pizza".to_owned(),
                description: None,
                icon_url: None,
            })),
            oracle_source: "social".to_owned(),
            title: "Pizza Bet".to_owned(),
            description: Description::plain("Bet a pizza"),
            resolution: Resolution::new("Winner decided by the group host"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            trading_access: vec![TradingAccess::Restricted {
                groups: vec![group],
            }],
            engine_id: None,
            locale: None,
        }
    }

    fn cleanup() {
        SOCIAL_CREATION_LOG.with(|l| l.borrow_mut().clear());
        SOCIAL_LIMITS.with(|l| *l.borrow_mut() = SocialLimits::default());
        SERIES_STORE.with(|s| s.borrow_mut().clear());
        GROUPS_STORE.with(|s| s.borrow_mut().clear());
    }

    /// Inserts a group into `GROUPS_STORE` with the given members. Used by tests
    /// to exercise visibility rules without going through the `create_group`
    /// update entrypoint (which depends on `msg_caller()` / `time()`).
    fn insert_group(group_id: GroupId, creator: Principal, members: &[Principal]) {
        let mut member_set = BTreeSet::new();
        for m in members {
            member_set.insert(*m);
        }
        let group = Group {
            group_id: group_id.clone(),
            name: format!("test-{}", group_id.as_str()),
            description: None,
            icon_url: None,
            creator,
            admins: BTreeSet::new(),
            members: member_set,
            created_at_ns: 0,
            updated_at_ns: 0,
            updated_by: creator,
        };
        GROUPS_STORE.with(|s| {
            s.borrow_mut().insert(group_id, group);
        });
    }

    fn list_all_visible_to(caller: Principal) -> Vec<SeriesId> {
        list_series_with_impl(
            ListSeriesParams {
                pagination: Some(PaginationParams {
                    limit: None,
                    cursor: None,
                }),
                ..Default::default()
            },
            caller,
            // `now = 0` with the default (unset) `only_unexpired` filter leaves
            // every series visible regardless of expiry.
            0,
        )
        .items
        .into_iter()
        .map(|s| s.series_id)
        .collect()
    }

    // --- Creator tier ---

    #[test]
    fn creator_unsupported_payout_unit() {
        cleanup();
        let caller = test_principal(1);

        let mut params = base_params();
        params.payout_unit = PayoutUnit::Fiat(FiatUnit::Eur);
        let result = add_as_creator(params, caller, 1_000_000_000);
        assert!(matches!(
            result,
            AddSeriesResult::Err(SeriesError::UnsupportedPayoutUnit)
        ));
    }

    #[test]
    fn creator_can_create_open_usd_series() {
        cleanup();
        let caller = test_principal(1);

        let result = add_as_creator(base_params(), caller, 1_000_000_000);
        assert!(matches!(result, AddSeriesResult::Ok(_)));
    }

    #[test]
    fn creator_can_create_series_with_valid_locale() {
        cleanup();
        let caller = test_principal(1);

        let mut params = base_params();
        params.locale = Some("it-IT".to_owned());
        let result = add_as_creator(params, caller, 1_000_000_000);
        let AddSeriesResult::Ok(id) = result else {
            panic!("expected Ok, got {result:?}");
        };

        let stored = SERIES_STORE.with(|s| s.borrow().get(&id).cloned()).unwrap();
        assert_eq!(stored.locale.as_deref(), Some("it-IT"));
    }

    #[test]
    fn creator_rejects_malformed_locale() {
        cleanup();
        let caller = test_principal(1);

        let mut params = base_params();
        params.locale = Some("english".to_owned());
        let result = add_as_creator(params, caller, 1_000_000_000);
        assert!(matches!(
            result,
            AddSeriesResult::Err(SeriesError::InvalidLocale)
        ));
    }

    #[test]
    fn fork_inherits_locale_from_source_when_unspecified() {
        cleanup();
        let caller = test_principal(1);
        let group = GroupId::from("grp_locale_inherit".to_owned());

        let mut params = base_params();
        params.locale = Some("es".to_owned());
        let AddSeriesResult::Ok(source_id) = add_as_creator(params, caller, 1_000_000_000) else {
            panic!("source create failed");
        };

        let fork_res = fork_as_creator(
            ForkSeriesParams {
                source_series_id: source_id,
                title: None,
                description: None,
                resolution: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group],
                }],
                engine_id: None,
                locale: None,
            },
            caller,
            2_000_000_000,
        );
        let AddSeriesResult::Ok(fork_id) = fork_res else {
            panic!("fork failed");
        };

        let forked = SERIES_STORE
            .with(|s| s.borrow().get(&fork_id).cloned())
            .unwrap();
        assert_eq!(forked.locale.as_deref(), Some("es"));
    }

    #[test]
    fn fork_can_override_locale() {
        cleanup();
        let caller = test_principal(1);
        let group = GroupId::from("grp_locale_override".to_owned());

        let mut params = base_params();
        params.locale = Some("en".to_owned());
        let AddSeriesResult::Ok(source_id) = add_as_creator(params, caller, 1_000_000_000) else {
            panic!("source create failed");
        };

        let fork_res = fork_as_creator(
            ForkSeriesParams {
                source_series_id: source_id,
                title: Some("Mercato in italiano".to_owned()),
                description: None,
                resolution: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group],
                }],
                engine_id: None,
                locale: Some("it".to_owned()),
            },
            caller,
            2_000_000_000,
        );
        let AddSeriesResult::Ok(fork_id) = fork_res else {
            panic!("fork failed");
        };

        let forked = SERIES_STORE
            .with(|s| s.borrow().get(&fork_id).cloned())
            .unwrap();
        assert_eq!(forked.locale.as_deref(), Some("it"));
    }

    #[test]
    fn creator_can_create_social_series() {
        cleanup();
        let caller = test_principal(1);

        let result = add_as_creator(social_params(2000), caller, 1_000_000_000);
        assert!(matches!(result, AddSeriesResult::Ok(_)));
    }

    // --- Fork tests ---

    #[test]
    fn fork_creates_distinct_series() {
        cleanup();
        let caller = test_principal(1);
        let group = GroupId::from("grp_1".to_owned());

        let res = add_as_creator(base_params(), caller, 1_000_000_000);
        let AddSeriesResult::Ok(source_id) = res else {
            panic!("Expected Ok");
        };

        let fork_params = ForkSeriesParams {
            source_series_id: source_id.clone(),
            title: Some("Forked Test".to_owned()),
            description: None,
            resolution: None,
            trading_access: vec![TradingAccess::Restricted {
                groups: vec![group],
            }],
            engine_id: None,
            locale: None,
        };

        let fork_res = fork_as_creator(fork_params, caller, 2_000_000_000);
        let fork_id = match fork_res {
            AddSeriesResult::Ok(id) => id,
            AddSeriesResult::Err(e) => panic!("Fork failed: {e:?}"),
        };

        assert_ne!(source_id, fork_id);

        let forked = SERIES_STORE
            .with(|s| s.borrow().get(&fork_id).cloned())
            .unwrap();
        assert_eq!(forked.forked_from, Some(source_id));
        assert_eq!(forked.title, "Forked Test");
    }

    #[test]
    fn fork_source_not_found() {
        cleanup();
        let caller = test_principal(1);

        let fork_params = ForkSeriesParams {
            source_series_id: SeriesId::from("nonexistent".to_owned()),
            title: None,
            description: None,
            resolution: None,
            trading_access: vec![TradingAccess::Restricted {
                groups: vec![GroupId::from("grp_1".to_owned())],
            }],
            engine_id: None,
            locale: None,
        };

        let res = fork_as_creator(fork_params, caller, 1_000_000_000);
        assert!(matches!(
            res,
            AddSeriesResult::Err(SeriesError::SourceSeriesNotFound)
        ));
    }

    #[test]
    fn multiple_forks_from_same_source_produce_unique_ids() {
        cleanup();
        let caller = test_principal(1);
        let group = GroupId::from("grp_multi".to_owned());

        let res = add_as_creator(base_params(), caller, 1_000_000_000);
        let AddSeriesResult::Ok(source_id) = res else {
            panic!("Expected Ok");
        };

        let fork1 = fork_as_creator(
            ForkSeriesParams {
                source_series_id: source_id.clone(),
                title: Some("Fork 1".to_owned()),
                description: None,
                resolution: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group.clone()],
                }],
                engine_id: None,
                locale: None,
            },
            caller,
            2_000_000_000,
        );
        let AddSeriesResult::Ok(fork1_id) = fork1 else {
            panic!("Fork 1 failed");
        };

        let fork2 = fork_as_creator(
            ForkSeriesParams {
                source_series_id: source_id.clone(),
                title: Some("Fork 2".to_owned()),
                description: None,
                resolution: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group],
                }],
                engine_id: None,
                locale: None,
            },
            caller,
            3_000_000_000,
        );
        let AddSeriesResult::Ok(fork2_id) = fork2 else {
            panic!("Fork 2 failed");
        };

        assert_ne!(fork1_id, fork2_id, "Multiple forks must produce unique IDs");
        assert_ne!(fork1_id, source_id);
        assert_ne!(fork2_id, source_id);
    }

    #[test]
    fn fork_limit_per_user_per_source() {
        cleanup();
        let caller = test_principal(1);
        let group = GroupId::from("grp_limit".to_owned());

        let res = add_as_creator(base_params(), caller, 1_000_000_000);
        let AddSeriesResult::Ok(source_id) = res else {
            panic!("Expected Ok");
        };

        for i in 0..MAX_FORKS_PER_SOURCE_PER_USER {
            let fork_res = fork_as_creator(
                ForkSeriesParams {
                    source_series_id: source_id.clone(),
                    title: None,
                    description: None,
                    resolution: None,
                    trading_access: vec![TradingAccess::Restricted {
                        groups: vec![group.clone()],
                    }],
                    engine_id: None,
                    locale: None,
                },
                caller,
                2_000_000_000 + i,
            );
            assert!(
                matches!(fork_res, AddSeriesResult::Ok(_)),
                "Fork {i} should succeed"
            );
        }

        let over_limit = fork_as_creator(
            ForkSeriesParams {
                source_series_id: source_id,
                title: None,
                description: None,
                resolution: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group],
                }],
                engine_id: None,
                locale: None,
            },
            caller,
            9_000_000_000,
        );
        assert!(
            matches!(
                over_limit,
                AddSeriesResult::Err(SeriesError::ForkLimitReached)
            ),
            "Should reject fork beyond limit"
        );
    }

    // --- Social tier ---

    #[test]
    fn any_user_can_create_social_market() {
        cleanup();
        let caller = test_principal(42);

        let result = add_as_social(social_params(3000), caller, 1_000_000_000);
        assert!(matches!(result, AddSeriesResult::Ok(_)));
    }

    #[test]
    fn social_market_must_be_restricted() {
        cleanup();
        let caller = test_principal(42);

        let mut params = social_params(4000);
        params.trading_access = vec![TradingAccess::Open];
        let result = add_as_social(params, caller, 1_000_000_000);
        assert!(matches!(
            result,
            AddSeriesResult::Err(SeriesError::SocialMarketMustBeRestricted)
        ));
    }

    #[test]
    fn social_hourly_rate_limit() {
        cleanup();
        let caller = test_principal(42);
        SOCIAL_LIMITS.with(|l| {
            *l.borrow_mut() = SocialLimits {
                max_per_hour: 2,
                max_per_user: 100,
            };
        });

        let now = 10_000_000_000_u64;

        let r1 = add_as_social(social_params(5000), caller, now);
        assert!(matches!(r1, AddSeriesResult::Ok(_)));

        let r2 = add_as_social(social_params(5001), caller, now + 1);
        assert!(matches!(r2, AddSeriesResult::Ok(_)));

        let r3 = add_as_social(social_params(5002), caller, now + 2);
        assert!(matches!(
            r3,
            AddSeriesResult::Err(SeriesError::SocialRateLimitExceeded)
        ));
    }

    #[test]
    fn social_per_user_total_limit() {
        cleanup();
        let caller = test_principal(42);
        SOCIAL_LIMITS.with(|l| {
            *l.borrow_mut() = SocialLimits {
                max_per_hour: 100,
                max_per_user: 2,
            };
        });

        let hour_ns = HOUR_NS;

        let r1 = add_as_social(social_params(6000), caller, 1_000_000_000);
        assert!(matches!(r1, AddSeriesResult::Ok(_)));

        let r2 = add_as_social(social_params(6001), caller, 1_000_000_000 + hour_ns + 1);
        assert!(matches!(r2, AddSeriesResult::Ok(_)));

        let r3 = add_as_social(social_params(6002), caller, 1_000_000_000 + 2 * hour_ns + 1);
        assert!(matches!(
            r3,
            AddSeriesResult::Err(SeriesError::SocialMaxPerUserReached)
        ));
    }

    #[test]
    fn social_rate_limit_resets_after_hour() {
        cleanup();
        let caller = test_principal(42);
        SOCIAL_LIMITS.with(|l| {
            *l.borrow_mut() = SocialLimits {
                max_per_hour: 1,
                max_per_user: 100,
            };
        });

        let hour_ns = HOUR_NS;
        let now = 10_000_000_000_u64;

        let r1 = add_as_social(social_params(7000), caller, now);
        assert!(matches!(r1, AddSeriesResult::Ok(_)));

        let r2 = add_as_social(social_params(7001), caller, now + 1);
        assert!(matches!(
            r2,
            AddSeriesResult::Err(SeriesError::SocialRateLimitExceeded)
        ));

        let r3 = add_as_social(social_params(7002), caller, now + hour_ns + 1);
        assert!(matches!(r3, AddSeriesResult::Ok(_)));
    }

    #[test]
    fn social_limits_are_per_user() {
        cleanup();
        let alice = test_principal(42);
        let bob = test_principal(43);
        SOCIAL_LIMITS.with(|l| {
            *l.borrow_mut() = SocialLimits {
                max_per_hour: 1,
                max_per_user: 100,
            };
        });

        let now = 10_000_000_000_u64;

        let r1 = add_as_social(social_params(8000), alice, now);
        assert!(matches!(r1, AddSeriesResult::Ok(_)));

        let r2 = add_as_social(social_params(8001), bob, now);
        assert!(matches!(r2, AddSeriesResult::Ok(_)));
    }

    // --- Social-tier fork ---

    #[test]
    fn any_user_can_fork_social_market() {
        cleanup();
        let creator = test_principal(40);
        let challenger = test_principal(41);
        let group = GroupId::from("grp_challenge".to_owned());

        let source_res = add_as_social(social_params(5000), creator, 1_000_000_000);
        let AddSeriesResult::Ok(source_id) = source_res else {
            panic!("source social create failed");
        };

        let fork_res = fork_as_social(
            ForkSeriesParams {
                source_series_id: source_id.clone(),
                title: Some("Friends challenge".to_owned()),
                description: None,
                resolution: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group],
                }],
                engine_id: None,
                locale: None,
            },
            challenger,
            2_000_000_000,
        );
        let AddSeriesResult::Ok(fork_id) = fork_res else {
            panic!("social fork should succeed");
        };

        let forked = SERIES_STORE
            .with(|s| s.borrow().get(&fork_id).cloned())
            .unwrap();
        assert_eq!(forked.forked_from, Some(source_id));
        assert_eq!(forked.creator, challenger);
    }

    #[test]
    fn social_fork_respects_hourly_rate_limit() {
        cleanup();
        let creator = test_principal(50);
        let challenger = test_principal(51);
        let group = GroupId::from("grp_rate".to_owned());

        SOCIAL_LIMITS.with(|l| {
            *l.borrow_mut() = SocialLimits {
                max_per_hour: 1,
                max_per_user: 100,
            };
        });

        let source_res = add_as_social(social_params(6000), creator, 1_000_000_000);
        let AddSeriesResult::Ok(source_id) = source_res else {
            panic!("source create failed");
        };

        let r1 = fork_as_social(
            ForkSeriesParams {
                source_series_id: source_id.clone(),
                title: None,
                description: None,
                resolution: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group.clone()],
                }],
                engine_id: None,
                locale: None,
            },
            challenger,
            1_500_000_000,
        );
        assert!(matches!(r1, AddSeriesResult::Ok(_)));

        let r2 = fork_as_social(
            ForkSeriesParams {
                source_series_id: source_id,
                title: None,
                description: None,
                resolution: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group],
                }],
                engine_id: None,
                locale: None,
            },
            challenger,
            1_500_000_001,
        );
        assert!(matches!(
            r2,
            AddSeriesResult::Err(SeriesError::SocialRateLimitExceeded)
        ));
    }

    // --- list_series visibility ---

    #[test]
    fn list_series_hides_restricted_from_non_members() {
        cleanup();
        let alice = test_principal(10);
        let bob = test_principal(11);
        let stranger = test_principal(99);
        let group_id = GroupId::from("grp_visibility".to_owned());
        insert_group(group_id.clone(), alice, &[alice, bob]);

        let open_res = add_as_creator(base_params(), alice, 1_000_000_000);
        let AddSeriesResult::Ok(open_id) = open_res else {
            panic!("open create failed");
        };

        let mut restricted_params = social_params(2000);
        restricted_params.trading_access = vec![TradingAccess::Restricted {
            groups: vec![group_id],
        }];
        let restricted_res = add_as_creator(restricted_params, alice, 1_500_000_000);
        let AddSeriesResult::Ok(restricted_id) = restricted_res else {
            panic!("restricted create failed");
        };

        let stranger_visible = list_all_visible_to(stranger);
        assert!(stranger_visible.contains(&open_id));
        assert!(!stranger_visible.contains(&restricted_id));

        let member_visible = list_all_visible_to(bob);
        assert!(member_visible.contains(&open_id));
        assert!(member_visible.contains(&restricted_id));
    }

    #[test]
    fn list_series_shows_restricted_to_creator_not_in_group() {
        cleanup();
        let creator = test_principal(20);
        let member = test_principal(21);
        let group_id = GroupId::from("grp_creator_visibility".to_owned());
        insert_group(group_id.clone(), creator, &[member]);

        let mut params = social_params(3000);
        params.trading_access = vec![TradingAccess::Restricted {
            groups: vec![group_id],
        }];
        let res = add_as_creator(params, creator, 1_000_000_000);
        let AddSeriesResult::Ok(id) = res else {
            panic!("create failed");
        };

        let creator_visible = list_all_visible_to(creator);
        assert!(
            creator_visible.contains(&id),
            "creator must see their own restricted series even when not a group member"
        );

        let member_visible = list_all_visible_to(member);
        assert!(member_visible.contains(&id));

        let stranger_visible = list_all_visible_to(test_principal(99));
        assert!(!stranger_visible.contains(&id));
    }

    // --- only_unexpired filter ---

    /// Lists series for `caller` with `only_unexpired = Some(true)` evaluated at
    /// `now`, returning the matching ids.
    fn list_unexpired_at(caller: Principal, now: u64) -> Vec<SeriesId> {
        list_series_with_impl(
            ListSeriesParams {
                only_unexpired: Some(true),
                pagination: Some(PaginationParams {
                    limit: None,
                    cursor: None,
                }),
                ..Default::default()
            },
            caller,
            now,
        )
        .items
        .into_iter()
        .map(|s| s.series_id)
        .collect()
    }

    #[test]
    fn only_unexpired_excludes_expired_series() {
        cleanup();
        let caller = test_principal(1);

        // Two open series with distinct expiries (base_params uses expiry 1000;
        // bump one so they get distinct ids).
        let mut early = base_params();
        early.expiry_ns = 1_000;
        let AddSeriesResult::Ok(early_id) = add_as_creator(early, caller, 0) else {
            panic!("early create failed");
        };

        let mut late = base_params();
        late.expiry_ns = 5_000;
        let AddSeriesResult::Ok(late_id) = add_as_creator(late, caller, 0) else {
            panic!("late create failed");
        };

        // now = 2000: early (1000) has expired, late (5000) is still open.
        let visible = list_unexpired_at(caller, 2_000);
        assert!(
            !visible.contains(&early_id),
            "series whose expiry is at/before now must be excluded"
        );
        assert!(
            visible.contains(&late_id),
            "series whose expiry is strictly after now must be included"
        );
    }

    #[test]
    fn only_unexpired_uses_strict_inequality_at_boundary() {
        cleanup();
        let caller = test_principal(1);

        let mut params = base_params();
        params.expiry_ns = 1_000;
        let AddSeriesResult::Ok(id) = add_as_creator(params, caller, 0) else {
            panic!("create failed");
        };

        // now exactly equal to expiry → expired (a series expires at expiry_ns).
        assert!(!list_unexpired_at(caller, 1_000).contains(&id));
        // now one nanosecond earlier → still open.
        assert!(list_unexpired_at(caller, 999).contains(&id));
    }

    #[test]
    fn unset_only_unexpired_preserves_legacy_behavior() {
        cleanup();
        let caller = test_principal(1);

        let mut params = base_params();
        params.expiry_ns = 1_000;
        let AddSeriesResult::Ok(id) = add_as_creator(params, caller, 0) else {
            panic!("create failed");
        };

        // Default params (only_unexpired = None) return the series even when
        // `now` is far past its expiry — the historical contract.
        let visible: Vec<SeriesId> =
            list_series_with_impl(ListSeriesParams::default(), caller, 1_000_000_000)
                .items
                .into_iter()
                .map(|s| s.series_id)
                .collect();
        assert!(visible.contains(&id));
    }

    // --- update_series_metadata ---

    /// Adds a series via the creator tier and returns its id. Panics on failure.
    fn seed_series(creator: Principal) -> SeriesId {
        let AddSeriesResult::Ok(id) = add_as_creator(base_params(), creator, 0) else {
            panic!("seed_series: add failed");
        };
        id
    }

    /// Reads a series out of the store, cloning it. Panics if absent.
    fn fetch(id: &SeriesId) -> Series {
        SERIES_STORE.with(|s| s.borrow().get(id).cloned().expect("series present"))
    }

    fn empty_update(id: SeriesId) -> UpdateSeriesMetadataParams {
        UpdateSeriesMetadataParams {
            series_id: id,
            description: None,
            icon_url: None,
            banner_url: None,
            locale: None,
        }
    }

    #[test]
    fn creator_can_update_metadata_and_critical_fields_are_untouched() {
        cleanup();
        let creator = test_principal(1);
        let id = seed_series(creator);
        let before = fetch(&id);

        let params = UpdateSeriesMetadataParams {
            description: Some(Description::plain("A short context line")),
            icon_url: Some(Some("https://example.test/icon.png".to_owned())),
            banner_url: Some(Some("https://example.test/banner.png".to_owned())),
            locale: Some(Some("en-US".to_owned())),
            ..empty_update(id.clone())
        };

        let updated = update_series_metadata_impl(params, creator, false).expect("update ok");

        assert_eq!(updated.description.plain, "A short context line");
        assert_eq!(
            updated.icon_url.as_deref(),
            Some("https://example.test/icon.png")
        );
        assert_eq!(
            updated.banner_url.as_deref(),
            Some("https://example.test/banner.png")
        );
        assert_eq!(updated.locale.as_deref(), Some("en-US"));

        // Identity and trust-critical fields must be preserved verbatim.
        assert_eq!(updated.series_id, before.series_id);
        assert_eq!(updated.title, before.title);
        assert_eq!(updated.resolution.clause, before.resolution.clause);
        assert_eq!(updated.creator, before.creator);
        assert_eq!(updated.expiry_ns, before.expiry_ns);

        // Persisted, not just returned.
        assert_eq!(fetch(&id).description.plain, "A short context line");
    }

    #[test]
    fn controller_can_update_metadata_even_when_not_creator() {
        cleanup();
        let creator = test_principal(1);
        let stranger = test_principal(99);
        let id = seed_series(creator);

        let params = UpdateSeriesMetadataParams {
            description: Some(Description::plain("Controller edit")),
            ..empty_update(id.clone())
        };

        // caller_is_controller = true overrides the creator check.
        update_series_metadata_impl(params, stranger, true).expect("controller update ok");
        assert_eq!(fetch(&id).description.plain, "Controller edit");
    }

    #[test]
    fn engine_admin_can_update_metadata() {
        cleanup();
        ENGINE_STORE.with(|s| s.borrow_mut().clear());

        let creator = test_principal(1);
        let admin = test_principal(2);
        let eid = EngineId::from("eng_test".to_owned());

        let engine = Engine {
            engine_id: eid.clone(),
            name: "Test Engine".to_owned(),
            description: None,
            icon_url: None,
            creator,
            admins: BTreeSet::from([admin]),
            allowed_roles: BTreeSet::new(),
            role_grants: Vec::new(),
            social_limits: None,
            created_at_ns: 0,
            updated_at_ns: 0,
            updated_by: creator,
        };
        ENGINE_STORE.with(|s| s.borrow_mut().insert(eid.clone(), engine));

        let id = seed_series(creator);
        SERIES_STORE.with(|s| {
            s.borrow_mut().get_mut(&id).unwrap().engine_id = Some(eid.clone());
        });

        let params = UpdateSeriesMetadataParams {
            description: Some(Description::plain("Engine admin edit")),
            ..empty_update(id.clone())
        };

        // admin is neither controller nor creator — authorized via the engine.
        update_series_metadata_impl(params, admin, false).expect("engine admin update ok");
        assert_eq!(fetch(&id).description.plain, "Engine admin edit");

        ENGINE_STORE.with(|s| s.borrow_mut().clear());
    }

    #[test]
    fn unauthorized_caller_is_rejected() {
        cleanup();
        let creator = test_principal(1);
        let stranger = test_principal(99);
        let id = seed_series(creator);

        let params = UpdateSeriesMetadataParams {
            description: Some(Description::plain("nope")),
            ..empty_update(id.clone())
        };

        let err = update_series_metadata_impl(params, stranger, false).unwrap_err();
        assert!(matches!(err, SeriesError::Unauthorized));
        // Unchanged.
        assert_eq!(fetch(&id).description.plain, "Test");
    }

    #[test]
    fn unknown_series_is_rejected() {
        cleanup();
        let params = empty_update(SeriesId::from("does_not_exist".to_owned()));
        let err = update_series_metadata_impl(params, test_principal(1), true).unwrap_err();
        assert!(matches!(err, SeriesError::SeriesNotFound));
    }

    #[test]
    fn description_too_long_is_rejected() {
        cleanup();
        let creator = test_principal(1);
        let id = seed_series(creator);

        let too_long = "x".repeat(MAX_SERIES_DESCRIPTION_LEN + 1);
        let params = UpdateSeriesMetadataParams {
            description: Some(Description::plain(too_long)),
            ..empty_update(id.clone())
        };

        let err = update_series_metadata_impl(params, creator, false).unwrap_err();
        assert!(matches!(err, SeriesError::DescriptionTooLong));
    }

    #[test]
    fn invalid_locale_is_rejected() {
        cleanup();
        let creator = test_principal(1);
        let id = seed_series(creator);

        let params = UpdateSeriesMetadataParams {
            locale: Some(Some("not a locale!!".to_owned())),
            ..empty_update(id.clone())
        };

        let err = update_series_metadata_impl(params, creator, false).unwrap_err();
        assert!(matches!(err, SeriesError::InvalidLocale));
    }

    #[test]
    fn some_none_clears_a_nullable_field_and_none_leaves_unchanged() {
        cleanup();
        let creator = test_principal(1);
        let id = seed_series(creator);

        // First set a banner and locale.
        update_series_metadata_impl(
            UpdateSeriesMetadataParams {
                banner_url: Some(Some("https://example.test/b.png".to_owned())),
                locale: Some(Some("es".to_owned())),
                ..empty_update(id.clone())
            },
            creator,
            false,
        )
        .expect("set ok");

        // Now clear banner (Some(None)) while leaving locale untouched (None).
        let updated = update_series_metadata_impl(
            UpdateSeriesMetadataParams {
                banner_url: Some(None),
                ..empty_update(id.clone())
            },
            creator,
            false,
        )
        .expect("clear ok");

        assert_eq!(updated.banner_url, None);
        assert_eq!(updated.locale.as_deref(), Some("es"));
    }
}
