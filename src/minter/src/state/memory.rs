use core::cell::RefCell;

use shared::types::minter::Config;

thread_local! {
    pub static CONFIG: RefCell<Option<Config>> = const { RefCell::new(None) };
}
