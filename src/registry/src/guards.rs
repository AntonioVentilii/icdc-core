use candid::Principal;
use ic_cdk::api::{is_controller, msg_caller};
use shared::types::{EngineId, EngineRole};

use crate::memory::ENGINE_STORE;

/// Guard function to ensure the caller is not anonymous.
pub fn caller_is_not_anonymous() -> Result<(), String> {
    if msg_caller() == Principal::anonymous() {
        return Err("Update call error. RejectionCode: CanisterReject, Error: Anonymous caller not authorised.".to_owned());
    }

    Ok(())
}

/// Guard function to ensure the caller is one of the canister controllers.
pub fn caller_is_controller() -> Result<(), String> {
    let caller = msg_caller();

    if is_controller(&caller) {
        Ok(())
    } else {
        Err("Caller is not a controller.".to_owned())
    }
}

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
                    .any(|grant| &grant.principal == principal && &grant.role == role)
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
                    .any(|grant| &grant.principal == principal && &grant.role == role)
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
