use std::collections::BTreeSet;

use candid::Principal;
use ic_cdk::api::{is_controller, msg_caller, time};
use ic_cdk_macros::{query, update};
use shared::types::{
    groups::{
        CreateGroupParams, CreateGroupResult, Group, GroupError, GroupId, GroupResult,
        UpdateGroupAdminsParams, UpdateGroupMembersParams, UpdateGroupParams,
        UpdateTradingAccessParams,
    },
    SeriesId, TradingAccess,
};

use crate::{
    guards::caller_is_not_anonymous,
    memory::{GROUPS_STORE, NEXT_GROUP_ID, SERIES_STORE},
};

/// Maximum number of characters allowed in a group name.
const MAX_GROUP_NAME_LEN: usize = 128;

/// Returns `true` if the caller has administrative privileges on the group.
///
/// A caller is considered a group admin if any of the following holds:
/// - They are a canister controller.
/// - They are the group's original creator.
/// - They are explicitly listed in the group's `admins` set.
fn caller_is_group_admin(group: &Group, caller: &Principal) -> bool {
    is_controller(caller) || *caller == group.creator || group.admins.contains(caller)
}

/// Stamps the audit fields on a group after a mutation.
fn stamp_audit(group: &mut Group, caller: Principal) {
    group.updated_at_ns = time();
    group.updated_by = caller;
}

// ---------------------------------------------------------------------------
// Group lifecycle
// ---------------------------------------------------------------------------

/// Creates a new trading group (closed circle).
///
/// The caller's principal is recorded as the group creator and is automatically
/// inserted as the first member. The creator is implicitly an admin and does
/// not need to be in the `admins` set. A monotonically increasing ID
/// (`grp_0`, `grp_1`, ...) is assigned.
///
/// # Errors
///
/// * [`GroupError::NameTooLong`] — if `params.name` exceeds [`MAX_GROUP_NAME_LEN`] chars.
///
/// # Access
///
/// Any authenticated (non-anonymous) caller.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn create_group(params: CreateGroupParams) -> CreateGroupResult {
    let caller = msg_caller();
    let now = time();

    if params.name.chars().count() > MAX_GROUP_NAME_LEN {
        return Err(GroupError::NameTooLong).into();
    }

    let group_id = NEXT_GROUP_ID.with(|id| {
        let mut id = id.borrow_mut();
        let current = *id;
        *id += 1;
        GroupId::from(format!("grp_{current}"))
    });

    let mut members = BTreeSet::new();
    members.insert(caller);

    let group = Group {
        group_id: group_id.clone(),
        name: params.name,
        description: params.description,
        icon_url: params.icon_url,
        creator: caller,
        admins: BTreeSet::new(),
        members,
        created_at_ns: now,
        updated_at_ns: now,
        updated_by: caller,
    };

    GROUPS_STORE.with(|store| {
        store.borrow_mut().insert(group_id.clone(), group);
    });

    Ok(group_id).into()
}

/// Updates a group's metadata (name, description, icon URL).
///
/// Fields set to `None` are left unchanged. For `description` and `icon_url`,
/// `Some(None)` clears the value while `Some(Some(..))` sets it.
///
/// # Errors
///
/// * [`GroupError::GroupNotFound`] — the `group_id` does not exist.
/// * [`GroupError::Unauthorized`] — the caller is not a group admin.
/// * [`GroupError::NameTooLong`] — if a new name exceeds [`MAX_GROUP_NAME_LEN`] chars.
///
/// # Access
///
/// Group admin (creator, explicit admin, or canister controller).
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn update_group(params: UpdateGroupParams) -> GroupResult {
    let caller = msg_caller();

    if let Some(ref name) = params.name {
        if name.chars().count() > MAX_GROUP_NAME_LEN {
            return Err(GroupError::NameTooLong).into();
        }
    }

    GROUPS_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let group = store
                .get_mut(&params.group_id)
                .ok_or(GroupError::GroupNotFound)?;

            if !caller_is_group_admin(group, &caller) {
                return Err(GroupError::Unauthorized);
            }

            if let Some(name) = params.name {
                group.name = name;
            }
            if let Some(description) = params.description {
                group.description = description;
            }
            if let Some(icon_url) = params.icon_url {
                group.icon_url = icon_url;
            }

            stamp_audit(group, caller);
            Ok(true)
        })
        .into()
}

/// Permanently deletes a group and all its membership data.
///
/// **Note:** deleting a group does not automatically update series that
/// reference it in their `trading_access`. Those series will simply fail
/// the membership check for the deleted group (the group ID will not
/// resolve to any members).
///
/// # Errors
///
/// * [`GroupError::GroupNotFound`] — the `group_id` does not exist.
/// * [`GroupError::Unauthorized`] — the caller is not a group admin.
///
/// # Access
///
/// Group admin (creator, explicit admin, or canister controller).
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn delete_group(group_id: GroupId) -> GroupResult {
    let caller = msg_caller();

    GROUPS_STORE
        .with(move |store| {
            let mut store = store.borrow_mut();
            let group = store.get(&group_id).ok_or(GroupError::GroupNotFound)?;

            if !caller_is_group_admin(group, &caller) {
                return Err(GroupError::Unauthorized);
            }

            store.remove(&group_id);
            Ok(true)
        })
        .into()
}

// ---------------------------------------------------------------------------
// Admin management
// ---------------------------------------------------------------------------

/// Adds one or more principals to an existing group's admin set.
///
/// Duplicate principals are silently ignored (the admin set is a `BTreeSet`).
///
/// # Errors
///
/// * [`GroupError::GroupNotFound`] — the `group_id` does not exist.
/// * [`GroupError::Unauthorized`] — the caller is not a group admin.
///
/// # Access
///
/// Group admin (creator, explicit admin, or canister controller).
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn add_group_admins(params: UpdateGroupAdminsParams) -> GroupResult {
    let caller = msg_caller();

    GROUPS_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let group = store
                .get_mut(&params.group_id)
                .ok_or(GroupError::GroupNotFound)?;

            if !caller_is_group_admin(group, &caller) {
                return Err(GroupError::Unauthorized);
            }

            for p in params.principals {
                group.admins.insert(p);
            }

            stamp_audit(group, caller);
            Ok(true)
        })
        .into()
}

/// Removes one or more principals from an existing group's admin set.
///
/// Principals that are not currently admins are silently ignored.
/// The group creator cannot be removed from admins (they are implicitly
/// always an admin).
///
/// # Errors
///
/// * [`GroupError::GroupNotFound`] — the `group_id` does not exist.
/// * [`GroupError::Unauthorized`] — the caller is not a group admin.
///
/// # Access
///
/// Group admin (creator, explicit admin, or canister controller).
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn remove_group_admins(params: UpdateGroupAdminsParams) -> GroupResult {
    let caller = msg_caller();
    let UpdateGroupAdminsParams {
        group_id,
        principals,
    } = params;

    GROUPS_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let group = store.get_mut(&group_id).ok_or(GroupError::GroupNotFound)?;

            if !caller_is_group_admin(group, &caller) {
                return Err(GroupError::Unauthorized);
            }

            for p in &principals {
                group.admins.remove(p);
            }

            stamp_audit(group, caller);
            Ok(true)
        })
        .into()
}

// ---------------------------------------------------------------------------
// Member management
// ---------------------------------------------------------------------------

/// Adds one or more principals to an existing group's member list.
///
/// Duplicate principals are silently ignored (the member set is a `BTreeSet`).
///
/// # Errors
///
/// * [`GroupError::GroupNotFound`] — the `group_id` does not exist.
/// * [`GroupError::Unauthorized`] — the caller is not a group admin.
///
/// # Access
///
/// Group admin (creator, explicit admin, or canister controller).
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn add_group_members(params: UpdateGroupMembersParams) -> GroupResult {
    let caller = msg_caller();

    GROUPS_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let group = store
                .get_mut(&params.group_id)
                .ok_or(GroupError::GroupNotFound)?;

            if !caller_is_group_admin(group, &caller) {
                return Err(GroupError::Unauthorized);
            }

            for p in params.principals {
                group.members.insert(p);
            }

            stamp_audit(group, caller);
            Ok(true)
        })
        .into()
}

/// Removes one or more principals from an existing group's member list.
///
/// Principals that are not currently members are silently ignored.
///
/// # Errors
///
/// * [`GroupError::GroupNotFound`] — the `group_id` does not exist.
/// * [`GroupError::Unauthorized`] — the caller is not a group admin.
///
/// # Access
///
/// Group admin (creator, explicit admin, or canister controller).
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn remove_group_members(params: UpdateGroupMembersParams) -> GroupResult {
    let caller = msg_caller();
    let UpdateGroupMembersParams {
        group_id,
        principals,
    } = params;

    GROUPS_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let group = store.get_mut(&group_id).ok_or(GroupError::GroupNotFound)?;

            if !caller_is_group_admin(group, &caller) {
                return Err(GroupError::Unauthorized);
            }

            for p in &principals {
                group.members.remove(p);
            }

            stamp_audit(group, caller);
            Ok(true)
        })
        .into()
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// Retrieves a group by its ID, returning `None` if it does not exist.
///
/// # Access
///
/// Public query — any caller.
#[query]
#[must_use]
pub fn get_group(group_id: GroupId) -> Option<Group> {
    GROUPS_STORE.with(move |store| store.borrow().get(&group_id).cloned())
}

/// Lists all registered groups, optionally filtered by creator principal.
///
/// # Arguments
///
/// * `creator` — if `Some`, only groups created by this principal are returned. If `None`, all
///   groups are returned.
///
/// # Access
///
/// Public query — any caller.
#[query]
#[must_use]
pub fn list_groups(creator: Option<Principal>) -> Vec<Group> {
    GROUPS_STORE.with(|store| {
        let store = store.borrow();
        match creator {
            Some(c) => store.values().filter(|g| g.creator == c).cloned().collect(),
            None => store.values().cloned().collect(),
        }
    })
}

/// Checks whether a principal is a member of a specific group.
///
/// Returns `false` if the group does not exist or if the principal is not a member.
///
/// # Access
///
/// Public query — any caller.
#[query]
#[must_use]
pub fn is_group_member(group_id: GroupId, principal: Principal) -> bool {
    GROUPS_STORE.with(move |store| {
        store
            .borrow()
            .get(&group_id)
            .is_some_and(|g| g.members.contains(&principal))
    })
}

// ---------------------------------------------------------------------------
// Trading access
// ---------------------------------------------------------------------------

/// Determines whether a principal is authorized to trade on a given series.
///
/// This is the central authorization query used by the clearing canister
/// (via inter-canister call) before accepting an order on a restricted series.
///
/// # Authorization rules (evaluated in order)
///
/// 1. **Controllers** are always authorized, regardless of policies.
/// 2. If the series does not exist, returns `false`.
/// 3. Each policy in the list is evaluated as a logical OR:
///    - [`TradingAccess::Open`] → immediately `true`.
///    - [`TradingAccess::Restricted`] → `true` if the principal is a member of **at least one** of
///      the referenced groups.
/// 4. If no policy grants access, returns `false`.
///
/// **Note:** `trading_access` must never be empty — the API layer enforces this.
///
/// # Access
///
/// Public query — any caller. Called by the clearing canister during
/// `submit_limit_order` / `submit_market_order` for restricted series.
#[query]
#[must_use]
pub fn is_trading_authorized(principal: Principal, series_id: SeriesId) -> bool {
    if is_controller(&principal) {
        return true;
    }

    let policies = SERIES_STORE.with(move |store| {
        store
            .borrow()
            .get(&series_id)
            .map(|s| s.trading_access.clone())
    });

    let Some(policies) = policies else {
        return false;
    };

    for policy in &policies {
        match policy {
            TradingAccess::Open => return true,
            TradingAccess::Restricted { groups } => {
                let authorized = GROUPS_STORE.with(|store| {
                    let store = store.borrow();
                    groups.iter().any(|gid| {
                        store
                            .get(gid)
                            .is_some_and(|g| g.members.contains(&principal))
                    })
                });
                if authorized {
                    return true;
                }
            }
        }
    }

    false
}

/// Atomically replaces the trading access policies on an existing series.
///
/// The entire `trading_access` vector is overwritten. To make a series open,
/// pass `[Open]`. To restrict it, pass one or more `Restricted { groups }` entries.
///
/// **Important:** the clearing canister caches series data. After updating
/// trading access, the clearing cache will pick up the change on the next
/// `ensure_series_registered` cache miss for that series.
///
/// # Errors
///
/// * [`GroupError::EmptyTradingAccess`] — the provided list is empty.
/// * [`GroupError::Unauthorized`] — the caller is not a canister controller.
/// * [`GroupError::SeriesNotFound`] — the series ID does not exist.
///
/// # Access
///
/// Canister controller only.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn update_trading_access(params: UpdateTradingAccessParams) -> GroupResult {
    let caller = msg_caller();

    if !is_controller(&caller) {
        return Err(GroupError::Unauthorized).into();
    }

    if params.trading_access.is_empty() {
        return Err(GroupError::EmptyTradingAccess).into();
    }

    SERIES_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let series = store
                .get_mut(&params.series_id)
                .ok_or(GroupError::SeriesNotFound)?;

            series.trading_access = params.trading_access;
            Ok(true)
        })
        .into()
}
