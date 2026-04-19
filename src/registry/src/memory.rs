use core::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use candid::Principal;
use ic_cdk::{storage, trap};
use shared::types::{
    Engine, EngineId, EngineRole, Group, GroupId, Oracle, RoleGrant, Series, SeriesId, SocialLimits,
};

thread_local! {
    /// Global stable storage for all registered derivative series.
    pub static SERIES_STORE: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };
    /// Global stable storage for authorised oracles and their metadata.
    pub static ORACLE_STORE: RefCell<BTreeMap<String, Oracle>> = const { RefCell::new(BTreeMap::new()) };
    /// Global stable storage for trading groups (closed circles).
    pub static GROUPS_STORE: RefCell<BTreeMap<GroupId, Group>> = const { RefCell::new(BTreeMap::new()) };
    /// Monotonic counter for generating unique [`GroupId`] values.
    pub static NEXT_GROUP_ID: RefCell<u64> = const { RefCell::new(0) };
    /// Per-principal creation timestamps (nanoseconds) for social markets.
    pub static SOCIAL_CREATION_LOG: RefCell<BTreeMap<Principal, Vec<u64>>> = const { RefCell::new(BTreeMap::new()) };
    /// Controller-configurable rate limits for social market creation.
    pub static SOCIAL_LIMITS: RefCell<SocialLimits> = RefCell::new(SocialLimits::default());
    /// Global stable storage for registered Engines.
    pub static ENGINE_STORE: RefCell<BTreeMap<EngineId, Engine>> = const { RefCell::new(BTreeMap::new()) };
    /// Monotonic counter for generating unique [`EngineId`] values.
    pub static NEXT_ENGINE_ID: RefCell<u64> = const { RefCell::new(0) };
}

/// Latest stable state schema.
#[derive(candid::CandidType, serde::Deserialize)]
struct StableStateV4 {
    series: BTreeMap<SeriesId, Series>,
    oracles: BTreeMap<String, Oracle>,
    groups: BTreeMap<GroupId, Group>,
    next_group_id: u64,
    social_creation_log: BTreeMap<Principal, Vec<u64>>,
    social_limits: SocialLimits,
    engines: BTreeMap<EngineId, Engine>,
    next_engine_id: u64,
}

/// V3 stable state (pre-engines: flat creator/forker maps).
#[derive(candid::CandidType, serde::Deserialize)]
struct StableStateV3 {
    series: BTreeMap<SeriesId, Series>,
    oracles: BTreeMap<String, Oracle>,
    creators: BTreeMap<Principal, bool>,
    groups: BTreeMap<GroupId, Group>,
    next_group_id: u64,
    forkers: BTreeMap<Principal, bool>,
    social_creation_log: BTreeMap<Principal, Vec<u64>>,
    social_limits: SocialLimits,
}

/// V2 stable state (pre-forkers/social).
#[derive(candid::CandidType, serde::Deserialize)]
struct StableStateV2 {
    series: BTreeMap<SeriesId, Series>,
    oracles: BTreeMap<String, Oracle>,
    creators: BTreeMap<Principal, bool>,
    groups: BTreeMap<GroupId, Group>,
    next_group_id: u64,
}

/// Migrates V3 flat creator/forker maps into a "Legacy" Engine (`eng_0`).
fn migrate_v3_to_engines(
    creators: &BTreeMap<Principal, bool>,
    forkers: &BTreeMap<Principal, bool>,
) -> (BTreeMap<EngineId, Engine>, u64) {
    let all_principals: BTreeSet<Principal> =
        creators.keys().chain(forkers.keys()).copied().collect();

    if all_principals.is_empty() {
        return (BTreeMap::new(), 0);
    }

    let engine_id = EngineId::from("eng_0".to_owned());
    let role_grants: Vec<RoleGrant> = all_principals
        .iter()
        .map(|p| RoleGrant {
            principal: *p,
            role: EngineRole::Creator,
            granted_by: Principal::management_canister(),
            granted_at_ns: 0,
        })
        .collect();

    let engine = Engine {
        engine_id: engine_id.clone(),
        name: "Legacy".to_owned(),
        description: Some("Auto-migrated from flat creator/forker maps".to_owned()),
        icon_url: None,
        creator: Principal::management_canister(),
        admins: BTreeSet::new(),
        allowed_roles: BTreeSet::from([EngineRole::Creator]),
        role_grants,
        social_limits: None,
        created_at_ns: 0,
        updated_at_ns: 0,
        updated_by: Principal::management_canister(),
    };

    let mut engines = BTreeMap::new();
    engines.insert(engine_id, engine);
    (engines, 1)
}

pub fn save_state() {
    let state = StableStateV4 {
        series: SERIES_STORE.with(|s| s.borrow().clone()),
        oracles: ORACLE_STORE.with(|o| o.borrow().clone()),
        groups: GROUPS_STORE.with(|g| g.borrow().clone()),
        next_group_id: NEXT_GROUP_ID.with(|id| *id.borrow()),
        social_creation_log: SOCIAL_CREATION_LOG.with(|l| l.borrow().clone()),
        social_limits: SOCIAL_LIMITS.with(|l| l.borrow().clone()),
        engines: ENGINE_STORE.with(|e| e.borrow().clone()),
        next_engine_id: NEXT_ENGINE_ID.with(|id| *id.borrow()),
    };

    storage::stable_save((state,))
        .unwrap_or_else(|e| trap(format!("Failed to save to stable storage: {e:?}")));
}

pub fn restore_state() {
    // Try V4 (latest) first.
    let v4: Result<(StableStateV4,), String> = storage::stable_restore();
    if let Ok((state,)) = v4 {
        SERIES_STORE.with(|w| *w.borrow_mut() = state.series);
        ORACLE_STORE.with(|w| *w.borrow_mut() = state.oracles);
        GROUPS_STORE.with(|w| *w.borrow_mut() = state.groups);
        NEXT_GROUP_ID.with(|w| *w.borrow_mut() = state.next_group_id);
        SOCIAL_CREATION_LOG.with(|w| *w.borrow_mut() = state.social_creation_log);
        SOCIAL_LIMITS.with(|w| *w.borrow_mut() = state.social_limits);
        ENGINE_STORE.with(|w| *w.borrow_mut() = state.engines);
        NEXT_ENGINE_ID.with(|w| *w.borrow_mut() = state.next_engine_id);
        return;
    }

    // Fallback: V3 format (flat creator/forker maps → migrate to engines).
    let v3: Result<(StableStateV3,), String> = storage::stable_restore();
    if let Ok((state,)) = v3 {
        SERIES_STORE.with(|w| *w.borrow_mut() = state.series);
        ORACLE_STORE.with(|w| *w.borrow_mut() = state.oracles);
        GROUPS_STORE.with(|w| *w.borrow_mut() = state.groups);
        NEXT_GROUP_ID.with(|w| *w.borrow_mut() = state.next_group_id);
        SOCIAL_CREATION_LOG.with(|w| *w.borrow_mut() = state.social_creation_log);
        SOCIAL_LIMITS.with(|w| *w.borrow_mut() = state.social_limits);

        let (engines, next_id) = migrate_v3_to_engines(&state.creators, &state.forkers);
        ENGINE_STORE.with(|w| *w.borrow_mut() = engines);
        NEXT_ENGINE_ID.with(|w| *w.borrow_mut() = next_id);
        return;
    }

    // Fallback: V2 format (pre-forkers/social).
    let v2: Result<(StableStateV2,), String> = storage::stable_restore();
    if let Ok((state,)) = v2 {
        SERIES_STORE.with(|w| *w.borrow_mut() = state.series);
        ORACLE_STORE.with(|w| *w.borrow_mut() = state.oracles);
        GROUPS_STORE.with(|w| *w.borrow_mut() = state.groups);
        NEXT_GROUP_ID.with(|w| *w.borrow_mut() = state.next_group_id);

        let (engines, next_id) = migrate_v3_to_engines(&state.creators, &BTreeMap::new());
        ENGINE_STORE.with(|w| *w.borrow_mut() = engines);
        NEXT_ENGINE_ID.with(|w| *w.borrow_mut() = next_id);
        return;
    }

    // Fallback: legacy tuple format (pre-groups).
    let (series, oracles, creators): (
        BTreeMap<SeriesId, Series>,
        BTreeMap<String, Oracle>,
        BTreeMap<Principal, bool>,
    ) = storage::stable_restore()
        .unwrap_or_else(|e| trap(format!("Failed to restore from stable storage: {e:?}")));

    SERIES_STORE.with(|w| *w.borrow_mut() = series);
    ORACLE_STORE.with(|w| *w.borrow_mut() = oracles);

    let (engines, next_id) = migrate_v3_to_engines(&creators, &BTreeMap::new());
    ENGINE_STORE.with(|w| *w.borrow_mut() = engines);
    NEXT_ENGINE_ID.with(|w| *w.borrow_mut() = next_id);
}
