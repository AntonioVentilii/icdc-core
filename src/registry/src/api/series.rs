use core::ops::Bound;

use candid::Principal;
use ic_cdk::api::{msg_caller, time};
use ic_cdk_macros::{query, update};
use shared::{
    constants::{
        HOUR_NS, MAX_FORKS_PER_SOURCE_PER_USER, MAX_SERIES_DESCRIPTION_LEN, MAX_SERIES_TITLE_LEN,
    },
    types::{
        series::{
            AddSeriesParams, AddSeriesResult, ForkSeriesParams, ListSeriesParams, PaginationParams,
            Series, SeriesError, SeriesPage,
        },
        BalanceDomain, NonMonetaryUnit, PayoutUnit, SeriesId, SeriesIdParams, TradingAccess,
    },
};

use crate::{
    guards::{caller_is_not_anonymous, is_engine_creator},
    memory::{SERIES_STORE, SOCIAL_CREATION_LOG, SOCIAL_LIMITS},
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
/// 1. **Creators** (controllers + Engine `Creator` role holders): may create any series.
/// 2. **Any authenticated user**: may create **social** markets (`BalanceDomain::Social` +
///    `NonMonetary` payout) with `Restricted` trading access, subject to per-user rate limits.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn add_series(params: AddSeriesParams) -> AddSeriesResult {
    let caller = msg_caller();

    let is_social_market = params.balance_domain == BalanceDomain::Social
        && matches!(params.payout_unit, PayoutUnit::NonMonetary(_));

    let tier = if is_engine_creator(&caller) {
        CreationTier::Creator
    } else if is_social_market {
        CreationTier::Social
    } else {
        return Err(SeriesError::Unauthorized).into();
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
        outcomes,
        icon_url,
        banner_url,
        trading_access,
    } = params;

    // --- Tier-specific validation ---

    let is_social_market = balance_domain == BalanceDomain::Social
        && matches!(payout_unit, PayoutUnit::NonMonetary(_));

    match &tier {
        CreationTier::Creator => {
            if !is_social_market && payout_unit != PayoutUnit::usd() {
                return Err(SeriesError::UnsupportedPayoutUnit).into();
            }
        }
        CreationTier::Social => {
            if !matches!(payout_unit, PayoutUnit::NonMonetary(_)) {
                return Err(SeriesError::SocialMarketRequiresNonMonetaryPayout).into();
            }
            if !is_all_restricted(&trading_access) {
                return Err(SeriesError::SocialMarketMustBeRestricted).into();
            }
            if let PayoutUnit::NonMonetary(NonMonetaryUnit::Social(ref reward)) = payout_unit {
                if let Err(e) = reward.validate() {
                    return Err(e).into();
                }
            }
            if let Err(e) = check_social_rate_limits(&caller, now) {
                return Err(e).into();
            }
        }
    }

    // --- Common validation ---

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
        icon_url,
        banner_url,
        trading_access,
        engine_id: None,
        forked_from: None,
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
/// Only controllers and Engine Creators may fork series.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn fork_series(params: ForkSeriesParams) -> AddSeriesResult {
    let caller = msg_caller();
    let now = time();

    if !is_engine_creator(&caller) {
        return Err(SeriesError::Unauthorized).into();
    }

    if !is_all_restricted(&params.trading_access) {
        return Err(SeriesError::ForkMustBeRestricted).into();
    }

    fork_series_impl(params, caller, now)
}

fn fork_series_impl(params: ForkSeriesParams, caller: Principal, now: u64) -> AddSeriesResult {
    SERIES_STORE
        .with(|store| {
            let mut store = store.borrow_mut();

            let source = store
                .get(&params.source_series_id)
                .cloned()
                .ok_or(SeriesError::SourceSeriesNotFound)?;

            let title = params.title.unwrap_or_else(|| source.title.clone());
            let description = params
                .description
                .unwrap_or_else(|| source.description.clone());

            if title.chars().count() > MAX_SERIES_TITLE_LEN {
                return Err(SeriesError::TitleTooLong);
            }
            if description.plain.chars().count() > MAX_SERIES_DESCRIPTION_LEN {
                return Err(SeriesError::DescriptionTooLong);
            }

            let fork_index = store
                .values()
                .filter(|s| {
                    s.forked_from.as_ref() == Some(&source.series_id) && s.creator == caller
                })
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
                icon_url: source.icon_url,
                banner_url: source.banner_url,
                trading_access: params.trading_access,
                engine_id: None,
                forked_from: Some(source.series_id),
            };

            store.insert(series_id.clone(), series);
            Ok(series_id)
        })
        .into()
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

/// Retrieves a specific [`Series`] by its [`SeriesId`].
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

        let range = match cursor {
            Some(c) => store.range((Bound::Excluded(c), Bound::Unbounded)),
            None => store.range(..),
        };

        let iter = range.filter(|(_, s)| params.matches(s));

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
    use candid::Principal;
    use shared::{
        constants::{HOUR_NS, MAX_FORKS_PER_SOURCE_PER_USER},
        types::{
            groups::GroupId, BalanceDomain, Description, FiatUnit, NonMonetaryUnit, PayoffType,
            PayoutUnit, SocialLimits, SocialReward, TradingAccess,
        },
    };

    use super::{add_series_impl, fork_series_impl, CreationTier};
    use crate::{
        memory::{SERIES_STORE, SOCIAL_CREATION_LOG, SOCIAL_LIMITS},
        AddSeriesParams, AddSeriesResult, ForkSeriesParams, SeriesError, SeriesId,
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
            outcomes: None,
            icon_url: None,
            banner_url: None,
            trading_access: vec![],
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
            outcomes: None,
            icon_url: None,
            banner_url: None,
            trading_access: vec![TradingAccess::Restricted {
                groups: vec![group],
            }],
        }
    }

    fn cleanup() {
        SOCIAL_CREATION_LOG.with(|l| l.borrow_mut().clear());
        SOCIAL_LIMITS.with(|l| *l.borrow_mut() = SocialLimits::default());
        SERIES_STORE.with(|s| s.borrow_mut().clear());
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
            trading_access: vec![TradingAccess::Restricted {
                groups: vec![group],
            }],
        };

        let fork_res = fork_series_impl(fork_params, caller, 2_000_000_000);
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
            trading_access: vec![TradingAccess::Restricted {
                groups: vec![GroupId::from("grp_1".to_owned())],
            }],
        };

        let res = fork_series_impl(fork_params, caller, 1_000_000_000);
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

        let fork1 = fork_series_impl(
            ForkSeriesParams {
                source_series_id: source_id.clone(),
                title: Some("Fork 1".to_owned()),
                description: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group.clone()],
                }],
            },
            caller,
            2_000_000_000,
        );
        let AddSeriesResult::Ok(fork1_id) = fork1 else {
            panic!("Fork 1 failed");
        };

        let fork2 = fork_series_impl(
            ForkSeriesParams {
                source_series_id: source_id.clone(),
                title: Some("Fork 2".to_owned()),
                description: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group],
                }],
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
            let fork_res = fork_series_impl(
                ForkSeriesParams {
                    source_series_id: source_id.clone(),
                    title: None,
                    description: None,
                    trading_access: vec![TradingAccess::Restricted {
                        groups: vec![group.clone()],
                    }],
                },
                caller,
                2_000_000_000 + i,
            );
            assert!(
                matches!(fork_res, AddSeriesResult::Ok(_)),
                "Fork {i} should succeed"
            );
        }

        let over_limit = fork_series_impl(
            ForkSeriesParams {
                source_series_id: source_id,
                title: None,
                description: None,
                trading_access: vec![TradingAccess::Restricted {
                    groups: vec![group],
                }],
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
}
