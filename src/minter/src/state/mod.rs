use shared::types::minter::{Config, UpgradeArg};

use crate::state::memory::CONFIG;

pub mod memory;

pub fn read_config() -> Result<Config, String> {
    CONFIG.with(|c| {
        c.borrow()
            .clone()
            .ok_or_else(|| "Config not initialised".to_owned())
    })
}

pub fn set_config(config: Config) {
    CONFIG.with(|c| {
        *c.borrow_mut() = Some(config);
    });
}

/// Applies an upgrade argument on top of the restored config, overriding only
/// the fields that are set. Fields left as `None` keep their persisted value.
pub fn apply_upgrade(arg: UpgradeArg) {
    CONFIG.with(|c| {
        if let Some(config) = c.borrow_mut().as_mut() {
            if let Some(ledger_canister) = arg.ledger_canister {
                config.ledger_canister = ledger_canister;
            }
            if let Some(authorized_callers) = arg.authorized_callers {
                config.authorized_callers = authorized_callers;
            }
        }
    });
}
