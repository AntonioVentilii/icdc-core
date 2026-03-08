use std::cell::RefCell;

use crate::Config;

thread_local! {
    pub static CONFIG: RefCell<Option<Config>> = const { RefCell::new(None) };
}
