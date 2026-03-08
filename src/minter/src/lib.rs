use std::cell::RefCell;

use candid::{CandidType, Nat, Principal};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MinterConfig {
    pub vusd_ledger: Principal,
    pub authorized_callers: Vec<Principal>,
}

thread_local! {
    static CONFIG: RefCell<Option<MinterConfig>> = RefCell::new(None);
}

#[init]
fn init(config: MinterConfig) {
    CONFIG.with(|c| *c.borrow_mut() = Some(config));
}

#[pre_upgrade]
fn pre_upgrade() {
    let config = CONFIG.with(|c| c.borrow().clone());
    ic_cdk::storage::stable_save((config,)).expect("Failed to save state");
}

#[post_upgrade]
fn post_upgrade() {
    let (config,): (Option<MinterConfig>,) =
        ic_cdk::storage::stable_restore().expect("Failed to restore state");
    CONFIG.with(|c| *c.borrow_mut() = config);
}

#[query]
fn get_config() -> Option<MinterConfig> {
    CONFIG.with(|c| c.borrow().clone())
}

#[update]
fn update_config(config: MinterConfig) {
    assert!(
        ic_cdk::api::is_controller(&ic_cdk::caller()),
        "Only controller can update config"
    );
    CONFIG.with(|c| *c.borrow_mut() = Some(config));
}

#[update]
async fn mint_vusd(to: Principal, amount: Nat) -> Result<Nat, String> {
    let config = CONFIG
        .with(|c| c.borrow().clone())
        .ok_or("Minter not configured")?;

    let caller = ic_cdk::caller();
    if !config.authorized_callers.contains(&caller) && !ic_cdk::api::is_controller(&caller) {
        return Err("Unauthorized".to_string());
    }

    // Call vUSD ledger to transfer from minting account (Self)
    let arg = icrc1_transfer_args {
        from_subaccount: None,
        to: record_to_account(to),
        amount: amount.clone(),
        fee: None,
        memo: None,
        created_at_time: None,
    };

    let (res,): (icrc1_transfer_result,) =
        ic_cdk::call(config.vusd_ledger, "icrc1_transfer", (arg,))
            .await
            .map_err(|(code, msg)| format!("Ledger call failed: {:?} - {}", code, msg))?;

    match res {
        icrc1_transfer_result::Ok(block) => Ok(block),
        icrc1_transfer_result::Err(e) => Err(format!("Transfer error: {:?}", e)),
    }
}

// ICRC-1 Types
#[allow(non_camel_case_types)]
#[derive(CandidType, Serialize, Deserialize, Debug)]
struct icrc1_transfer_args {
    from_subaccount: Option<vec_u8>,
    to: record_account,
    amount: Nat,
    fee: Option<Nat>,
    memo: Option<vec_u8>,
    created_at_time: Option<u64>,
}

#[allow(non_camel_case_types)]
type vec_u8 = Vec<u8>;

#[allow(non_camel_case_types)]
#[derive(CandidType, Serialize, Deserialize, Debug)]
struct record_account {
    owner: Principal,
    subaccount: Option<vec_u8>,
}

#[allow(non_camel_case_types)]
#[derive(CandidType, Serialize, Deserialize, Debug)]
enum icrc1_transfer_result {
    Ok(Nat),
    Err(icrc1_transfer_error),
}

#[allow(non_camel_case_types)]
#[derive(CandidType, Serialize, Deserialize, Debug)]
enum icrc1_transfer_error {
    BadFee { expected_fee: Nat },
    BadBurn { min_burn_amount: Nat },
    InsufficientFunds { balance: Nat },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    Duplicate { duplicate_of: Nat },
    TemporarilyUnavailable,
    GenericError { error_code: Nat, message: String },
}

fn record_to_account(p: Principal) -> record_account {
    record_account {
        owner: p,
        subaccount: None,
    }
}
