use candid::Principal;
use ic_cdk_macros::update;

use crate::{guards::caller_is_controller, memory::REGISTRY_CANISTER};

#[update(guard = "caller_is_controller")]
pub fn set_registry_canister(registry: Principal) {
    REGISTRY_CANISTER.with(|r| {
        *r.borrow_mut() = registry;
    });
}
