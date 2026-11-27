use ic_cdk::api::{is_controller, msg_caller};
pub use shared::guards::{caller_is_controller, caller_is_not_anonymous};

use crate::state::read_config;

/// Guard function to ensure the caller is either an allowed caller or a controller.
pub fn caller_is_authorized() -> Result<(), String> {
    let caller = msg_caller();

    let config = read_config()?;

    if config.authorized_callers.contains(&caller) || is_controller(&caller) {
        Ok(())
    } else {
        Err("Caller is not authorized.".to_owned())
    }
}
