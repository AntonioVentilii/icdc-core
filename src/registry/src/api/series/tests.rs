use std::collections::BTreeSet;

use candid::Principal;
use shared::{
    constants::{HOUR_NS, MAX_FORKS_PER_SOURCE_PER_USER, MAX_SERIES_DESCRIPTION_LEN},
    types::{
        engine::{Engine, EngineId},
        groups::GroupId,
        BalanceDomain, Description, FiatUnit, Group, NonMonetaryUnit, PayoffType, PayoutUnit,
        Price, Resolution, SocialLimits, SocialReward, TradingAccess,
    },
};

use super::{
    add_series_impl, fork_series_impl, list_series_with_impl, update_series_metadata_impl,
    CreationTier,
};
use crate::{
    memory::{ENGINE_STORE, GROUPS_STORE, SERIES_STORE, SOCIAL_CREATION_LOG, SOCIAL_LIMITS},
    AddSeriesParams, AddSeriesResult, ForkSeriesParams, ListSeriesParams, PaginationParams, Series,
    SeriesError, SeriesId, UpdateSeriesMetadataParams,
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
        start_ns: None,
        payoff_type: PayoffType::Binary,
        strike: None,
        settlement_cap: None,
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
        start_ns: None,
        payoff_type: PayoffType::Binary,
        strike: None,
        settlement_cap: None,
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
    // `now` is far past its expiry: the historical contract.
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

    // admin is neither controller nor creator; authorized via the engine.
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

// --- Linear (forward/NDF/future) validation ---

fn linear_params() -> AddSeriesParams {
    AddSeriesParams {
        payoff_type: PayoffType::Linear,
        strike: None,
        settlement_cap: Some(Price::new(20_0000_0000, 8)),
        ..base_params()
    }
}

#[test]
fn linear_happy_path() {
    cleanup();
    let res = add_as_creator(linear_params(), test_principal(1), 1_000_000_000);
    assert!(matches!(res, AddSeriesResult::Ok(_)));
}

#[test]
fn linear_requires_settlement_cap() {
    cleanup();
    let mut p = linear_params();
    p.settlement_cap = None;
    let res = add_as_creator(p, test_principal(1), 1_000_000_000);
    assert!(matches!(
        res,
        AddSeriesResult::Err(SeriesError::LinearRequiresSettlementCap)
    ));
}

#[test]
fn linear_rejects_zero_cap() {
    cleanup();
    let mut p = linear_params();
    p.settlement_cap = Some(Price::new(0, 8));
    let res = add_as_creator(p, test_principal(1), 1_000_000_000);
    assert!(matches!(
        res,
        AddSeriesResult::Err(SeriesError::InvalidSettlementCap)
    ));
}

#[test]
fn linear_rejects_strike() {
    cleanup();
    let mut p = linear_params();
    p.strike = Some(Price::new(8_0000_0000, 8));
    let res = add_as_creator(p, test_principal(1), 1_000_000_000);
    assert!(matches!(
        res,
        AddSeriesResult::Err(SeriesError::LinearRejectsStrike)
    ));
}

#[test]
fn linear_rejects_outcomes() {
    cleanup();
    let mut p = linear_params();
    p.outcomes = Some(Vec::new());
    let res = add_as_creator(p, test_principal(1), 1_000_000_000);
    assert!(matches!(
        res,
        AddSeriesResult::Err(SeriesError::LinearRejectsOutcomes)
    ));
}

#[test]
fn settlement_cap_only_for_linear() {
    cleanup();
    // base_params is a Binary series; a cap is meaningless there.
    let mut p = base_params();
    p.settlement_cap = Some(Price::new(20_0000_0000, 8));
    let res = add_as_creator(p, test_principal(1), 1_000_000_000);
    assert!(matches!(
        res,
        AddSeriesResult::Err(SeriesError::SettlementCapOnlyForLinear)
    ));
}

// --- start_ns (scheduled series) --------------------------------------

/// Lists series for `caller` with `tradeable_now = Some(true)` evaluated at
/// `now`, returning the matching ids.
fn list_tradeable_at(caller: Principal, now: u64) -> Vec<SeriesId> {
    list_series_with_impl(
        ListSeriesParams {
            tradeable_now: Some(true),
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

fn scheduled_params(start_ns: u64, expiry_ns: u64) -> AddSeriesParams {
    let mut params = base_params();
    params.expiry_ns = expiry_ns;
    params.start_ns = Some(start_ns);
    params
}

/// Lists series for `caller` with `due = Some(true)` evaluated at `now`,
/// returning the matching ids.
fn list_due_at(caller: Principal, now: u64) -> Vec<SeriesId> {
    list_series_with_impl(
        ListSeriesParams {
            due: Some(true),
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

/// The `due` filter returns only expired, resolvable markets (the exact
/// candidate set a resolution solver iterates), so the read scales with the
/// due subset rather than the whole registry.
#[test]
fn due_filter_returns_only_expired_series() {
    cleanup();
    let caller = test_principal(1);

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

    // now = 2000: early (1000) has expired and is due; late (5000) is not.
    let due = list_due_at(caller, 2_000);
    assert!(
        due.contains(&early_id),
        "an expired, resolvable series must be due"
    );
    assert!(
        !due.contains(&late_id),
        "a still-open series is not yet due"
    );
}

/// `None` is the only way to say "already live", so a start in the past (or
/// at exactly `now`) is rejected rather than silently normalized. Otherwise
/// the same market could be registered under two different ids.
#[test]
fn add_series_rejects_start_at_or_before_now() {
    cleanup();
    let caller = test_principal(1);

    let at_now = add_as_creator(scheduled_params(1_000, 5_000), caller, 1_000);
    assert!(
        matches!(at_now, AddSeriesResult::Err(SeriesError::StartNotInFuture)),
        "start exactly at now must be rejected, got {at_now:?}"
    );

    let in_past = add_as_creator(scheduled_params(1_000, 5_000), caller, 1_001);
    assert!(
        matches!(in_past, AddSeriesResult::Err(SeriesError::StartNotInFuture)),
        "start before now must be rejected, got {in_past:?}"
    );
}

#[test]
fn add_series_rejects_start_at_or_after_expiry() {
    cleanup();
    let caller = test_principal(1);

    let at_expiry = add_as_creator(scheduled_params(5_000, 5_000), caller, 0);
    assert!(
        matches!(
            at_expiry,
            AddSeriesResult::Err(SeriesError::StartNotBeforeExpiry)
        ),
        "a zero-width window must be rejected, got {at_expiry:?}"
    );

    let after_expiry = add_as_creator(scheduled_params(6_000, 5_000), caller, 0);
    assert!(
        matches!(
            after_expiry,
            AddSeriesResult::Err(SeriesError::StartNotBeforeExpiry)
        ),
        "start after expiry must be rejected, got {after_expiry:?}"
    );
}

#[test]
fn scheduled_series_stores_start_ns() {
    cleanup();
    let caller = test_principal(1);

    let AddSeriesResult::Ok(id) = add_as_creator(scheduled_params(1_000, 5_000), caller, 0) else {
        panic!("create failed");
    };

    let stored = SERIES_STORE
        .with(|s| s.borrow().get(&id).cloned())
        .expect("series stored");
    assert_eq!(stored.start_ns, Some(1_000));
}

/// `start_ns` is hashed into the id, so a scheduled series can never collide
/// with an otherwise-identical immediate one and inherit a start its creator
/// did not ask for.
#[test]
fn scheduled_and_immediate_series_get_distinct_ids() {
    cleanup();
    let caller = test_principal(1);

    let mut immediate = base_params();
    immediate.expiry_ns = 5_000;
    let AddSeriesResult::Ok(immediate_id) = add_as_creator(immediate, caller, 0) else {
        panic!("immediate create failed");
    };

    let AddSeriesResult::Ok(scheduled_id) =
        add_as_creator(scheduled_params(1_000, 5_000), caller, 0)
    else {
        panic!("scheduled create failed");
    };

    assert_ne!(immediate_id, scheduled_id);
}

#[test]
fn tradeable_now_excludes_series_before_its_start() {
    cleanup();
    let caller = test_principal(1);

    let AddSeriesResult::Ok(id) = add_as_creator(scheduled_params(1_000, 5_000), caller, 0) else {
        panic!("create failed");
    };

    assert!(
        !list_tradeable_at(caller, 999).contains(&id),
        "must be hidden one nanosecond before the open"
    );
    assert!(
        list_tradeable_at(caller, 1_000).contains(&id),
        "must be tradeable at exactly the open"
    );
    assert!(
        !list_tradeable_at(caller, 5_000).contains(&id),
        "must be hidden at expiry"
    );
}

/// A not-yet-open series is still unexpired: `only_unexpired` keeps its
/// original, wider meaning so existing callers see no change.
#[test]
fn only_unexpired_still_shows_upcoming_series() {
    cleanup();
    let caller = test_principal(1);

    let AddSeriesResult::Ok(id) = add_as_creator(scheduled_params(1_000, 5_000), caller, 0) else {
        panic!("create failed");
    };

    assert!(list_unexpired_at(caller, 999).contains(&id));
}

#[test]
fn fork_inherits_start_ns() {
    cleanup();
    let caller = test_principal(1);
    let group = GroupId::from("grp_1".to_owned());

    let AddSeriesResult::Ok(source_id) = add_as_creator(scheduled_params(1_000, 5_000), caller, 0)
    else {
        panic!("source create failed");
    };

    let fork_params = ForkSeriesParams {
        source_series_id: source_id,
        title: Some("Forked".to_owned()),
        description: None,
        resolution: None,
        trading_access: vec![TradingAccess::Restricted {
            groups: vec![group],
        }],
        engine_id: None,
        locale: None,
    };

    let AddSeriesResult::Ok(fork_id) = fork_as_creator(fork_params, caller, 500) else {
        panic!("fork failed");
    };

    let forked = SERIES_STORE
        .with(|s| s.borrow().get(&fork_id).cloned())
        .expect("fork stored");
    assert_eq!(forked.start_ns, Some(1_000));
}
