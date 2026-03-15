use candid::Principal;
use ic_cdk::{api::is_controller, caller};

use crate::state::read_config;

/// Guard function to ensure the caller is not anonymous.
pub fn caller_is_not_anonymous() -> Result<(), String> {
    if caller() == Principal::anonymous() {
        return Err("Update call error. RejectionCode: CanisterReject, Error: Anonymous caller not authorised.".to_owned());
    }

    Ok(())
}

/// Guard function to ensure the caller is one of the canister controllers.
pub fn caller_is_controller() -> Result<(), String> {
    let caller = caller();

    if is_controller(&caller) {
        Ok(())
    } else {
        Err("Caller is not a controller.".to_owned())
    }
}

/// Guard function to ensure the caller is either an allowed caller or a controller.
pub fn caller_is_authorized() -> Result<(), String> {
    let caller = caller();

    let config = read_config()?;

    if config.authorized_callers.contains(&caller) || is_controller(&caller) {
        Ok(())
    } else {
        Err("Caller is not authorized.".to_owned())
    }
}
