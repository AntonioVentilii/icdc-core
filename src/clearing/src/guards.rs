use candid::Principal;
use ic_cdk::{api::is_controller, caller};

/// Guard function to ensure the caller is not anonymous.
pub fn caller_is_not_anonymous() -> Result<(), String> {
    if caller() == Principal::anonymous() {
        Err("Update call error. RejectionCode: CanisterReject, Error: Anonymous caller not authorised.".to_owned())
    } else {
        Ok(())
    }
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
