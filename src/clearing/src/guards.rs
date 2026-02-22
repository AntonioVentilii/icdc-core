use candid::Principal;
use ic_cdk::{api::is_controller, caller};

pub fn caller_is_not_anonymous() -> Result<(), String> {
    if caller() == Principal::anonymous() {
        Err("Update call error. RejectionCode: CanisterReject, Error: Anonymous caller not authorized.".to_string())
    } else {
        Ok(())
    }
}

pub fn caller_is_controller() -> Result<(), String> {
    let caller = caller();
    if is_controller(&caller) {
        Ok(())
    } else {
        Err("Caller is not a controller.".to_string())
    }
}
