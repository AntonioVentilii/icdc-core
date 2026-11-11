//! Endpoint guards every canister uses, for the `guard = "..."` argument of
//! `#[update]` and `#[query]`. A guard specific to one canister stays in that
//! canister's `guards.rs`.

use candid::Principal;
use ic_cdk::api::{is_controller, msg_caller};

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
