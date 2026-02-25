use candid::Principal;
use ic_cdk_macros::update;

use crate::{guards::caller_is_controller, memory::REGISTRY_CANISTER};

/// Sets the principal of the Series Registry canister.
///
/// This principal is used to discover and validate derivative series.
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn set_registry_canister(registry: Principal) {
    REGISTRY_CANISTER.with(|r| {
        *r.borrow_mut() = registry;
    });
}
