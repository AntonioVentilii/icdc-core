use candid::Principal;
use ic_cdk::{api::is_controller, caller};

use crate::memory::AUTHORIZED_CREATORS;

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

/// Guard function to ensure the caller is an authorized series creator.
///
/// Controllers are always authorized.
pub fn caller_is_authorized_creator() -> Result<(), String> {
    let caller = caller();

    if is_controller(&caller) {
        return Ok(());
    }

    let is_authorized =
        AUTHORIZED_CREATORS.with(|a| return *a.borrow().get(&caller).unwrap_or(&false));

    if is_authorized {
        Ok(())
    } else {
        Err("Caller is not an authorized series creator.".to_owned())
    }
}
