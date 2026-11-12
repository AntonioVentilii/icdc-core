use ic_cdk_macros::{query, update};
use shared::types::SocialLimits;

use crate::{guards::caller_is_controller, memory::SOCIAL_LIMITS};

// ---------------------------------------------------------------------------
// Social market limits
// ---------------------------------------------------------------------------

/// Updates the rate limits for social (non-monetary) market creation.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn set_social_limits(limits: SocialLimits) {
    SOCIAL_LIMITS.with(|l| *l.borrow_mut() = limits);
}

/// Returns the current rate limits for social market creation.
#[query]
#[must_use]
pub fn get_social_limits() -> SocialLimits {
    SOCIAL_LIMITS.with(|l| l.borrow().clone())
}
