use core::cell::RefCell;
use std::collections::BTreeMap;

use candid::Principal;
use ic_cdk::{storage, trap};
use shared::types::{Group, GroupId, Oracle, Series, SeriesId};

thread_local! {
    /// Global stable storage for all registered derivative series.
    pub static SERIES_STORE: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };
    /// Global stable storage for authorised oracles and their metadata.
    pub static ORACLE_STORE: RefCell<BTreeMap<String, Oracle>> = const { RefCell::new(BTreeMap::new()) };
    /// Global stable storage for authorised series creators.
    pub static AUTHORIZED_CREATORS: RefCell<BTreeMap<Principal, bool>> = const { RefCell::new(BTreeMap::new()) };
    /// Global stable storage for trading groups (closed circles).
    ///
    /// Maps each [`GroupId`] to its [`Group`] definition (name, creator, members).
    /// Groups are referenced by series via [`TradingAccess::Restricted`] policies.
    pub static GROUPS_STORE: RefCell<BTreeMap<GroupId, Group>> = const { RefCell::new(BTreeMap::new()) };
    /// Monotonic counter for generating unique [`GroupId`] values.
    ///
    /// Incremented on each `create_group` call. The resulting ID is `"grp_{counter}"`.
    /// Persisted in stable storage across upgrades.
    pub static NEXT_GROUP_ID: RefCell<u64> = const { RefCell::new(0) };
}

/// Versioned stable state for forward-compatible upgrades.
///
/// Prior to groups support, the canister stored a bare tuple `(series, oracles, creators)`.
/// [`restore_state`] first tries to decode the new shape and falls back to the legacy tuple,
/// initialising the groups store to empty in that case.
#[derive(candid::CandidType, serde::Deserialize)]
struct StableState {
    series: BTreeMap<SeriesId, Series>,
    oracles: BTreeMap<String, Oracle>,
    creators: BTreeMap<Principal, bool>,
    groups: BTreeMap<GroupId, Group>,
    next_group_id: u64,
}

pub fn save_state() {
    let state = StableState {
        series: SERIES_STORE.with(|s| s.borrow().clone()),
        oracles: ORACLE_STORE.with(|o| o.borrow().clone()),
        creators: AUTHORIZED_CREATORS.with(|a| a.borrow().clone()),
        groups: GROUPS_STORE.with(|g| g.borrow().clone()),
        next_group_id: NEXT_GROUP_ID.with(|id| *id.borrow()),
    };

    storage::stable_save((state,))
        .unwrap_or_else(|e| trap(format!("Failed to save to stable storage: {e:?}")));
}

pub fn restore_state() {
    // Try versioned format first.
    let versioned: Result<(StableState,), String> = storage::stable_restore();
    if let Ok((state,)) = versioned {
        SERIES_STORE.with(|w| *w.borrow_mut() = state.series);
        ORACLE_STORE.with(|w| *w.borrow_mut() = state.oracles);
        AUTHORIZED_CREATORS.with(|w| *w.borrow_mut() = state.creators);
        GROUPS_STORE.with(|w| *w.borrow_mut() = state.groups);
        NEXT_GROUP_ID.with(|w| *w.borrow_mut() = state.next_group_id);
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
    AUTHORIZED_CREATORS.with(|w| *w.borrow_mut() = creators);
}
