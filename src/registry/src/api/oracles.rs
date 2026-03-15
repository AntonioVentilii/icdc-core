use candid::Principal;
use ic_cdk::{
    api::{is_controller, time},
    caller,
};
use ic_cdk_macros::{query, update};
use shared::types::Oracle;

use crate::{
    errors::OracleError,
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::ORACLE_STORE,
    params::{AddOracleParams, ManageOraclePrincipalsParams, UpdateOracleMetadataParams},
    results::OracleResult,
    utils::canonical_id_part,
};

/// Registers a new price oracle in the registry.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn add_oracle(params: AddOracleParams) -> OracleResult {
    let result: Result<(), OracleError> = {
        let caller = caller();

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
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn update_oracle_metadata(params: UpdateOracleMetadataParams) -> OracleResult {
    let result: Result<(), OracleError> = {
        let caller = caller();

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
#[must_use]
pub fn manage_oracle_principals(params: ManageOraclePrincipalsParams) -> OracleResult {
    let result: Result<(), OracleError> = {
        let caller = caller();

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
#[must_use]
#[expect(clippy::needless_pass_by_value)]
pub fn get_oracle(oracle_id: String) -> Option<Oracle> {
    let oracle_id: &str = oracle_id.as_str();
    ORACLE_STORE.with(|store| return store.borrow().get(oracle_id).cloned())
}

/// Checks if a principal is authorized to push settlement data for a given oracle.
#[query]
#[must_use]
#[expect(clippy::needless_pass_by_value)]
pub fn is_oracle_authorized(oracle_id: String, principal: Principal) -> bool {
    let oracle_id: &str = oracle_id.as_str();
    ORACLE_STORE.with(|store| {
        if let Some(oracle) = store.borrow().get(oracle_id) {
            oracle.authorized_principals.contains(&principal)
        } else {
            false
        }
    })
}
