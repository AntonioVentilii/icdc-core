use ic_cdk::api::is_controller;
use ic_cdk_macros::{query, update};

use crate::{
    errors::OracleError,
    guards::caller_is_not_anonymous,
    memory::ORACLE_STORE,
    params::{AddOracleParams, ManageOraclePrincipalsParams, UpdateOracleMetadataParams},
    results::OracleResult,
    utils::canonical_id_part,
};

/// Registers a new price oracle in the registry.
#[update(guard = "caller_is_not_anonymous")]
pub fn add_oracle(params: AddOracleParams) -> OracleResult {
    let result: Result<(), OracleError> = {
        let caller = ic_cdk::caller();
        if !is_controller(&caller) {
            return OracleResult::Err(OracleError::UnauthorizedOracleManager);
        }

        let oracle_id = canonical_id_part(&params.oracle_id);

        ORACLE_STORE.with(|store| {
            let mut store = store.borrow_mut();

            if store.contains_key(&oracle_id) {
                return Err(OracleError::OracleAlreadyExists);
            }

            let oracle = shared::types::Oracle {
                oracle_id: oracle_id.clone(),
                metadata: params.metadata,
                authorized_principals: params.authorized_principals.into_iter().collect(),
                manager: caller,
                registered_at_ns: ic_cdk::api::time(),
            };

            store.insert(oracle_id, oracle);
            Ok(())
        })
    };

    result.into()
}

/// Updates the metadata of an existing oracle.
#[update(guard = "caller_is_not_anonymous")]
pub fn update_oracle_metadata(params: UpdateOracleMetadataParams) -> OracleResult {
    let result: Result<(), OracleError> = {
        let caller = ic_cdk::caller();

        ORACLE_STORE.with(|store| {
            let mut store = store.borrow_mut();
            let oracle = store
                .get_mut(&params.oracle_id)
                .ok_or(OracleError::OracleNotFound)?;

            if !is_controller(&caller) && caller != oracle.manager {
                return Err(OracleError::UnauthorizedOracleManager);
            }

            oracle.metadata = params.metadata;
            Ok(())
        })
    };

    result.into()
}

/// Adds or removes authorised principals for an oracle.
#[update(guard = "caller_is_not_anonymous")]
pub fn manage_oracle_principals(params: ManageOraclePrincipalsParams) -> OracleResult {
    let result: Result<(), OracleError> = {
        let caller = ic_cdk::caller();

        ORACLE_STORE.with(|store| {
            let mut store = store.borrow_mut();
            let oracle = store
                .get_mut(&params.oracle_id)
                .ok_or(OracleError::OracleNotFound)?;

            if !is_controller(&caller) && caller != oracle.manager {
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
pub fn get_oracle(oracle_id: String) -> Option<shared::types::Oracle> {
    ORACLE_STORE.with(|store| store.borrow().get(&oracle_id).cloned())
}

/// Checks if a principal is authorized to push settlement data for a given oracle.
#[query]
pub fn is_oracle_authorized(oracle_id: String, principal: candid::Principal) -> bool {
    ORACLE_STORE.with(|store| {
        if let Some(oracle) = store.borrow().get(&oracle_id) {
            oracle.authorized_principals.contains(&principal)
        } else {
            false
        }
    })
}
