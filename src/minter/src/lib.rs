pub mod guards;
pub mod state;
pub mod utils;

use ic_cdk::{call::Call, export_candid, storage, trap};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use icrc_ledger_types::icrc1::transfer::{BlockIndex, TransferArg, TransferError};
use shared::types::minter::{Config, ConfigResult, MintParams, MintResult, MinterArg};

use crate::{
    guards::{caller_is_authorized, caller_is_controller, caller_is_not_anonymous},
    state::{apply_upgrade, memory::CONFIG, read_config, set_config},
    utils::to_account,
};

#[init]
fn init(arg: MinterArg) {
    match arg {
        MinterArg::Init(config) => set_config(config),
        MinterArg::Upgrade(_) => {
            trap("cannot install minter with an Upgrade argument; expected Init")
        }
    }
}

#[pre_upgrade]
fn pre_upgrade() {
    let config = CONFIG.with(|c| return c.borrow().clone());

    storage::stable_save((config,)).expect("Failed to save state");
}

#[post_upgrade]
fn post_upgrade(arg: MinterArg) {
    let (config,): (Option<Config>,) = storage::stable_restore().expect("Failed to restore state");

    CONFIG.with(|c| *c.borrow_mut() = config);

    match arg {
        MinterArg::Upgrade(Some(upgrade)) => apply_upgrade(upgrade),
        MinterArg::Upgrade(None) => {}
        MinterArg::Init(_) => trap("cannot upgrade minter with an Init argument; expected Upgrade"),
    }
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
    match mint_impl(params).await {
        Ok(outcome) => outcome,
        Err(msg) => MintResult::Err(msg),
    }
}

async fn mint_impl(params: MintParams) -> Result<MintResult, String> {
    let config = read_config()?;

    let arg = TransferArg {
        from_subaccount: None,
        to: to_account(params.to),
        amount: params.amount.clone(),
        fee: None,
        memo: None,
        created_at_time: None,
    };

    let response = Call::bounded_wait(config.ledger_canister, "icrc1_transfer")
        .with_args(&(arg,))
        .await
        .map_err(|e| format!("Ledger call failed: {e}"))?;

    let (res,) = response
        .candid_tuple::<(Result<BlockIndex, TransferError>,)>()
        .map_err(|e| format!("Ledger response decode failed: {e}"))?;

    Ok(res.into())
}

export_candid!();
