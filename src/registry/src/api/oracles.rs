use candid::Principal;
use ic_cdk::api::{is_controller, msg_caller, time};
use ic_cdk_macros::{query, update};
use shared::types::oracle::{
    AddOracleParams, ManageOraclePrincipalsParams, Oracle, OracleError, OracleResult,
    UpdateOracleMetadataParams,
};

use crate::{
    guards::{caller_is_not_anonymous, is_engine_oracle_admin},
    memory::ORACLE_STORE,
    utils::canonical_id_part,
};

/// Registers a new price oracle in the registry.
///
/// Controllers and Engine `OracleAdmin` role holders may register oracles.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn add_oracle(params: AddOracleParams) -> OracleResult {
    let caller = msg_caller();

    if !is_engine_oracle_admin(&caller) {
        return Err(OracleError::UnauthorizedOracleManager).into();
    }

    let result: Result<(), OracleError> = {
        let oracle_id = canonical_id_part(&params.oracle_id);

        ORACLE_STORE.with(|store| {
            let mut store = store.borrow_mut();

            if store.contains_key(&oracle_id) {
                return Err(OracleError::OracleAlreadyExists);
            }

            let oracle = Oracle {
                oracle_id: oracle_id.clone(),
                metadata: params.metadata,
                authorized_principals: params.authorized_principals.into_iter().collect(),
                manager: caller,
                registered_at_ns: time(),
            };

            store.insert(oracle_id, oracle);
            Ok(())
        })
    };

    result.into()
}

/// Updates the metadata of an existing oracle.
///
/// Controllers, the oracle's manager, and Engine `OracleAdmin` role holders may update metadata.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn update_oracle_metadata(params: UpdateOracleMetadataParams) -> OracleResult {
    let result: Result<(), OracleError> = {
        let caller = msg_caller();

        ORACLE_STORE.with(|store| {
            let mut store = store.borrow_mut();
            let oracle = store
                .get_mut(&params.oracle_id)
                .ok_or(OracleError::OracleNotFound)?;

            if !is_controller(&caller)
                && caller != oracle.manager
                && !is_engine_oracle_admin(&caller)
            {
                return Err(OracleError::UnauthorizedOracleManager);
            }

            oracle.metadata = params.metadata;
            Ok(())
        })
    };

    result.into()
}

/// Adds or removes authorised principals for an oracle.
///
/// Controllers, the oracle's manager, and Engine `OracleAdmin` role holders may manage principals.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn manage_oracle_principals(params: ManageOraclePrincipalsParams) -> OracleResult {
    let result: Result<(), OracleError> = {
        let caller = msg_caller();

        ORACLE_STORE.with(|store| {
            let mut store = store.borrow_mut();
            let oracle = store
                .get_mut(&params.oracle_id)
                .ok_or(OracleError::OracleNotFound)?;

            if !is_controller(&caller)
                && caller != oracle.manager
                && !is_engine_oracle_admin(&caller)
            {
                return Err(OracleError::UnauthorizedOracleManager);
            }

            for p in params.add_principals {
                oracle.authorized_principals.insert(p);
            }

            for p in params.remove_principals {
                oracle.authorized_principals.remove(&p);
            }

            Ok(())
        })
    };

    result.into()
}

/// Retrieves the details of a specific oracle by its ID.
#[query]
#[must_use]
pub fn get_oracle(oracle_id: String) -> Option<Oracle> {
    ORACLE_STORE.with(move |store| store.borrow().get(oracle_id.as_str()).cloned())
}

/// Checks if a principal is authorized to push settlement data for a given oracle.
#[query]
#[must_use]
pub fn is_oracle_authorized(oracle_id: String, principal: Principal) -> bool {
    ORACLE_STORE.with(move |store| {
        if let Some(oracle) = store.borrow().get(oracle_id.as_str()) {
            oracle.authorized_principals.contains(&principal)
        } else {
            false
        }
    })
}
