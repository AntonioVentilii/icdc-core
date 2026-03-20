use shared::types::minter::Config;

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
