use candid::Principal;
use ic_cdk::api::is_controller;
pub use shared::guards::{caller_is_controller, caller_is_not_anonymous};
use shared::types::{EngineId, EngineRole};

use crate::memory::ENGINE_STORE;

/// Returns `true` if the principal holds the given role in **any** registered Engine
/// whose `allowed_roles` still includes that role.
#[must_use]
pub fn has_engine_role(principal: &Principal, role: &EngineRole) -> bool {
    ENGINE_STORE.with(|store| {
        store.borrow().values().any(|engine| {
            engine.allowed_roles.contains(role)
                && engine
                    .role_grants
                    .iter()
                    .any(|grant| &grant.grantee == principal && &grant.role == role)
        })
    })
}

/// Returns `true` if the principal holds the given role on a **specific** Engine
/// whose `allowed_roles` still includes that role.
#[must_use]
pub fn has_engine_role_on(principal: &Principal, role: &EngineRole, engine_id: &EngineId) -> bool {
    ENGINE_STORE.with(|store| {
        store.borrow().get(engine_id).is_some_and(|engine| {
            engine.allowed_roles.contains(role)
                && engine
                    .role_grants
                    .iter()
                    .any(|grant| &grant.grantee == principal && &grant.role == role)
        })
    })
}

/// Returns `true` if the principal is an Engine Creator (controller or role holder).
#[must_use]
pub fn is_engine_creator(principal: &Principal) -> bool {
    if is_controller(principal) {
        return true;
    }
    has_engine_role(principal, &EngineRole::Creator)
}

/// Returns `true` if the principal is an Engine `OracleAdmin` (controller or role holder).
#[must_use]
pub fn is_engine_oracle_admin(principal: &Principal) -> bool {
    if is_controller(principal) {
        return true;
    }
    has_engine_role(principal, &EngineRole::OracleAdmin)
}
