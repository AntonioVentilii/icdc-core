use std::collections::BTreeSet;

use candid::Principal;
use ic_cdk::api::{is_controller, msg_caller, time};
use ic_cdk_macros::{query, update};
use shared::types::engine::{
    Engine, EngineError, EngineId, EngineResult, GrantEngineRoleParams, RegisterEngineParams,
    RegisterEngineResult, RevokeEngineRoleParams, RoleGrant, UpdateEngineAdminsParams,
    UpdateEngineAllowedRolesParams, UpdateEngineParams,
};

use crate::{
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{ENGINE_STORE, NEXT_ENGINE_ID},
};

const MAX_ENGINE_NAME_LEN: usize = 128;

fn is_engine_admin(engine: &Engine, principal: &Principal) -> bool {
    is_controller(principal) || *principal == engine.creator || engine.admins.contains(principal)
}

/// Registers a new Engine. Only canister controllers may call this.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn register_engine(params: RegisterEngineParams) -> RegisterEngineResult {
    let caller = msg_caller();
    let now = time();

    if params.name.chars().count() > MAX_ENGINE_NAME_LEN {
        return Err(EngineError::NameTooLong).into();
    }

    let name_taken = ENGINE_STORE.with(|store| {
        store
            .borrow()
            .values()
            .any(|engine| engine.name == params.name)
    });
    if name_taken {
        return Err(EngineError::EngineAlreadyExists).into();
    }

    let engine_id = NEXT_ENGINE_ID.with(|id| {
        let mut id = id.borrow_mut();
        let current = *id;
        *id += 1;
        EngineId::from(format!("eng_{current}"))
    });

    let mut admins: BTreeSet<Principal> = params.admins.into_iter().collect();
    admins.insert(caller);

    let engine = Engine {
        engine_id: engine_id.clone(),
        name: params.name,
        description: params.description,
        icon_url: params.icon_url,
        creator: caller,
        admins,
        allowed_roles: params.allowed_roles.into_iter().collect(),
        role_grants: Vec::new(),
        social_limits: None,
        created_at_ns: now,
        updated_at_ns: now,
        updated_by: caller,
    };

    ENGINE_STORE.with(|store| {
        store.borrow_mut().insert(engine_id.clone(), engine);
    });

    Ok(engine_id).into()
}

/// Updates an Engine's metadata. Engine admins or controllers may call this.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn update_engine(params: UpdateEngineParams) -> EngineResult {
    let caller = msg_caller();
    let now = time();

    ENGINE_STORE
        .with(|store| {
            let mut store = store.borrow_mut();

            {
                let engine = store
                    .get(&params.engine_id)
                    .ok_or(EngineError::EngineNotFound)?;
                if !is_engine_admin(engine, &caller) {
                    return Err(EngineError::Unauthorized);
                }
                if let Some(ref name) = params.name {
                    if name.chars().count() > MAX_ENGINE_NAME_LEN {
                        return Err(EngineError::NameTooLong);
                    }
                    let conflict = store
                        .iter()
                        .any(|(id, e)| id != &params.engine_id && e.name == *name);
                    if conflict {
                        return Err(EngineError::EngineAlreadyExists);
                    }
                }
            }

            let engine = store.get_mut(&params.engine_id).unwrap();

            if let Some(name) = params.name {
                engine.name = name;
            }

            if let Some(desc) = params.description {
                engine.description = desc;
            }

            if let Some(icon) = params.icon_url {
                engine.icon_url = icon;
            }

            engine.updated_at_ns = now;
            engine.updated_by = caller;
            Ok(())
        })
        .into()
}

/// Updates the allowed roles for an Engine. Only controllers may call this.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn update_engine_allowed_roles(params: UpdateEngineAllowedRolesParams) -> EngineResult {
    let caller = msg_caller();
    let now = time();

    ENGINE_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let engine = store
                .get_mut(&params.engine_id)
                .ok_or(EngineError::EngineNotFound)?;

            engine.allowed_roles = params.allowed_roles.into_iter().collect();
            engine.updated_at_ns = now;
            engine.updated_by = caller;
            Ok(())
        })
        .into()
}

/// Grants a role to a principal within an Engine.
///
/// The role must be in the Engine's `allowed_roles` set. Only Engine admins
/// or controllers may grant roles.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn grant_engine_role(params: GrantEngineRoleParams) -> EngineResult {
    let caller = msg_caller();
    let now = time();

    ENGINE_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let engine = store
                .get_mut(&params.engine_id)
                .ok_or(EngineError::EngineNotFound)?;

            if !is_engine_admin(engine, &caller) {
                return Err(EngineError::Unauthorized);
            }

            if !engine.allowed_roles.contains(&params.role) {
                return Err(EngineError::RoleNotAllowed);
            }

            let already_granted = engine
                .role_grants
                .iter()
                .any(|g| g.principal == params.principal && g.role == params.role);
            if already_granted {
                return Err(EngineError::RoleAlreadyGranted);
            }

            engine.role_grants.push(RoleGrant {
                principal: params.principal,
                role: params.role,
                granted_by: caller,
                granted_at_ns: now,
            });

            engine.updated_at_ns = now;
            engine.updated_by = caller;
            Ok(())
        })
        .into()
}

/// Revokes a role from a principal within an Engine.
///
/// Only Engine admins or controllers may revoke roles.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn revoke_engine_role(params: RevokeEngineRoleParams) -> EngineResult {
    let caller = msg_caller();
    let now = time();
    let RevokeEngineRoleParams {
        engine_id,
        principal,
        role,
    } = params;

    ENGINE_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let engine = store
                .get_mut(&engine_id)
                .ok_or(EngineError::EngineNotFound)?;

            if !is_engine_admin(engine, &caller) {
                return Err(EngineError::Unauthorized);
            }

            let before = engine.role_grants.len();
            engine
                .role_grants
                .retain(|g| !(g.principal == principal && g.role == role));

            if engine.role_grants.len() == before {
                return Err(EngineError::RoleNotGranted);
            }

            engine.updated_at_ns = now;
            engine.updated_by = caller;
            Ok(())
        })
        .into()
}

/// Adds admin principals to an Engine. Engine admins or controllers may call this.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn add_engine_admins(params: UpdateEngineAdminsParams) -> EngineResult {
    let caller = msg_caller();
    let now = time();

    ENGINE_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let engine = store
                .get_mut(&params.engine_id)
                .ok_or(EngineError::EngineNotFound)?;

            if !is_engine_admin(engine, &caller) {
                return Err(EngineError::Unauthorized);
            }

            for p in params.principals {
                engine.admins.insert(p);
            }

            engine.updated_at_ns = now;
            engine.updated_by = caller;
            Ok(())
        })
        .into()
}

/// Removes admin principals from an Engine. Engine admins or controllers may call this.
///
/// The Engine creator cannot be removed as an admin.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn remove_engine_admins(params: UpdateEngineAdminsParams) -> EngineResult {
    let caller = msg_caller();
    let now = time();

    ENGINE_STORE
        .with(|store| {
            let mut store = store.borrow_mut();
            let engine = store
                .get_mut(&params.engine_id)
                .ok_or(EngineError::EngineNotFound)?;

            if !is_engine_admin(engine, &caller) {
                return Err(EngineError::Unauthorized);
            }

            for p in &params.principals {
                if *p == engine.creator {
                    return Err(EngineError::CannotRemoveCreator);
                }
            }

            for p in params.principals {
                engine.admins.remove(&p);
            }

            engine.updated_at_ns = now;
            engine.updated_by = caller;
            Ok(())
        })
        .into()
}

/// Retrieves an Engine by its ID.
#[query]
#[must_use]
pub fn get_engine(engine_id: EngineId) -> Option<Engine> {
    ENGINE_STORE.with(move |store| store.borrow().get(&engine_id).cloned())
}

/// Lists all registered Engines.
#[query]
#[must_use]
pub fn list_engines() -> Vec<Engine> {
    ENGINE_STORE.with(|store| store.borrow().values().cloned().collect())
}
