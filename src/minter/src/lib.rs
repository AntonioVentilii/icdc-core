use ic_cdk::{export_candid, storage};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use icrc_ledger_types::icrc1::transfer::{BlockIndex, TransferArg, TransferError};

use crate::{
    guards::{caller_is_authorized, caller_is_controller, caller_is_not_anonymous},
    state::{memory::CONFIG, read_config, set_config},
    types::{
        params::MintParams,
        results::{ConfigResult, MintResult},
        state::Config,
    },
    utils::to_account,
};

mod guards;

mod state;
mod types;
mod utils;

#[init]
fn init(config: Config) {
    set_config(config);
}

#[pre_upgrade]
fn pre_upgrade() {
    let config = CONFIG.with(|c| c.borrow().clone());

    storage::stable_save((config,)).expect("Failed to save state");
}

#[post_upgrade]
fn post_upgrade() {
    let (config,): (Option<Config>,) = storage::stable_restore().expect("Failed to restore state");

    CONFIG.with(|c| *c.borrow_mut() = config);
}

#[query(guard = "caller_is_not_anonymous")]
pub fn config() -> ConfigResult {
    read_config().into()
}

#[update(guard = "caller_is_controller")]
pub fn update_config(config: Config) {
    set_config(config);
}

#[update(guard = "caller_is_authorized")]
pub async fn mint(params: MintParams) -> MintResult {
    let config = match read_config() {
        Ok(config) => config,
        Err(err) => return MintResult::Err(err.to_string()),
    };

    let arg = TransferArg {
        from_subaccount: None,
        to: to_account(params.to),
        amount: params.amount.clone(),
        fee: None,
        memo: None,
        created_at_time: None,
    };

    let (res,): (Result<BlockIndex, TransferError>,) =
        match ic_cdk::call(config.ledger_canister, "icrc1_transfer", (arg,)).await {
            Ok(v) => v,
            Err((code, msg)) => {
                return MintResult::Err(format!("Ledger call failed: {:?} - {}", code, msg));
            }
        };

    res.into()
}

export_candid!();
