pub mod guards;
pub mod state;
pub mod utils;

use ic_cdk::{call::Call, export_candid, storage};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use icrc_ledger_types::icrc1::transfer::{BlockIndex, TransferArg, TransferError};
use shared::types::minter::{Config, ConfigResult, MintParams, MintResult};

use crate::{
    guards::{caller_is_authorized, caller_is_controller, caller_is_not_anonymous},
    state::{memory::CONFIG, read_config, set_config},
    utils::to_account,
};

#[init]
fn init(config: Config) {
    set_config(config);
}

#[pre_upgrade]
fn pre_upgrade() {
    let config = CONFIG.with(|c| return c.borrow().clone());

    storage::stable_save((config,)).expect("Failed to save state");
}

#[post_upgrade]
fn post_upgrade() {
    let (config,): (Option<Config>,) = storage::stable_restore().expect("Failed to restore state");

    CONFIG.with(|c| *c.borrow_mut() = config);
}

#[query(guard = "caller_is_not_anonymous")]
#[must_use]
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
        Err(err) => return MintResult::Err(err.clone()),
    };

    let arg = TransferArg {
        from_subaccount: None,
        to: to_account(params.to),
        amount: params.amount.clone(),
        fee: None,
        memo: None,
        created_at_time: None,
    };

    let transfer_response = match Call::bounded_wait(config.ledger_canister, "icrc1_transfer")
        .with_args(&(arg,))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return MintResult::Err(format!("Ledger call failed: {e}"));
        }
    };

    let (res,): (Result<BlockIndex, TransferError>,) =
        match transfer_response.candid_tuple::<(Result<BlockIndex, TransferError>,)>() {
            Ok(v) => v,
            Err(e) => {
                return MintResult::Err(format!("Ledger response decode failed: {e}"));
            }
        };

    res.into()
}

export_candid!();
