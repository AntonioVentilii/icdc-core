use candid::Principal;
use ic_cdk::api::is_controller;
use ic_cdk_macros::{query, update};

use crate::{guards::caller_is_controller, memory::AUTHORIZED_CREATORS};

/// Authorizes a list of principals to create new derivative series.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn add_authorized_creators(principals: Vec<Principal>) {
    AUTHORIZED_CREATORS.with(|a| {
        let mut a = a.borrow_mut();

        for p in principals {
            a.insert(p, true);
        }
    });
}

/// Removes authorization from a list of principals, preventing them from creating new series.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn remove_authorized_creators(principals: Vec<Principal>) {
    AUTHORIZED_CREATORS.with(|a| {
        let mut a = a.borrow_mut();

        for p in principals {
            a.remove(&p);
        }
    });
}

/// Returns a list of all principals currently authorized to create series.
///
/// This method is gated to canister controllers.
#[query(guard = "caller_is_controller")]
#[must_use]
pub fn list_authorized_creators() -> Vec<Principal> {
    AUTHORIZED_CREATORS.with(|a| return a.borrow().keys().copied().collect())
}

/// Checks if a principal is authorized to create derivative series.
#[query]
#[must_use]
pub fn is_authorized_creator(principal: Principal) -> bool {
    if is_controller(&principal) {
        return true;
    }

    AUTHORIZED_CREATORS.with(|a| return *a.borrow().get(&principal).unwrap_or(&false))
}
