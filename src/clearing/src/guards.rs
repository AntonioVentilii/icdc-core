use candid::Principal;
use ic_cdk::api::{is_controller, msg_caller};

use crate::memory::RISK_STATE;

/// Guard function to ensure the caller is not anonymous.
pub fn caller_is_not_anonymous() -> Result<(), String> {
    if msg_caller() == Principal::anonymous() {
        Err("Update call error. RejectionCode: CanisterReject, Error: Anonymous caller not authorised.".to_owned())
    } else {
        Ok(())
    }
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

/// Guard function to ensure the caller holds the risk administrator role.
///
/// A controller is never a risk administrator (`docs/risk/margin-model.md`, section 12): a
/// principal that became a controller after being granted the role is rejected here, so the two
/// roles never sit on the same key.
pub fn caller_is_risk_admin() -> Result<(), String> {
    let caller = msg_caller();

    if !is_controller(&caller)
        && RISK_STATE.with(|state| state.borrow().risk_admins.contains(&caller))
    {
        Ok(())
    } else {
        Err("Caller is not a risk administrator.".to_owned())
    }
}

/// Guard function to ensure the caller is a risk administrator or a canister controller.
pub fn caller_is_risk_admin_or_controller() -> Result<(), String> {
    caller_is_risk_admin()
        .or_else(|_| caller_is_controller())
        .map_err(|_| "Caller is neither a risk administrator nor a controller.".to_owned())
}
