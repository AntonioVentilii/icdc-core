use candid::{CandidType, Deserialize, Nat, Principal};
use ic_cdk::api::msg_caller;
use ic_cdk_macros::{query, update};
use icrc_ledger_types::icrc1::account::Account;
use icrc_ledger_types::icrc1::transfer::{TransferArg, TransferError};
use icrc_ledger_types::icrc2::transfer_from::{TransferFromArgs, TransferFromError};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct State {
    pub balances: BTreeMap<Account, u128>,
}

thread_local! {
    static STATE: std::cell::RefCell<State> = std::cell::RefCell::new(State::default());
}

#[update]
pub fn icrc1_transfer(args: TransferArg) -> Result<Nat, TransferError> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let from = Account {
            owner: msg_caller(),
            subaccount: args.from_subaccount,
        };
        let from_bal = s.balances.get(&from).cloned().unwrap_or(0);
        let amount = args.amount.0.try_into().unwrap_or(u128::MAX);
        
        if from_bal < amount {
            return Err(TransferError::InsufficientFunds { balance: Nat::from(from_bal) });
        }
        
        s.balances.insert(from, from_bal - amount);
        let to_bal = s.balances.get(&args.to).cloned().unwrap_or(0);
        s.balances.insert(args.to, to_bal + amount);
        
        Ok(Nat::from(1u64)) // Mock block index
    })
}

#[update]
pub fn icrc2_transfer_from(args: TransferFromArgs) -> Result<Nat, TransferFromError> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let from_bal = s.balances.get(&args.from).cloned().unwrap_or(0);
        let amount = args.amount.0.try_into().unwrap_or(u128::MAX);
        
        if from_bal < amount {
            return Err(TransferFromError::InsufficientFunds { balance: Nat::from(from_bal) });
        }
        
        s.balances.insert(args.from, from_bal - amount);
        let to_bal = s.balances.get(&args.to).cloned().unwrap_or(0);
        s.balances.insert(args.to, to_bal + amount);
        
        Ok(Nat::from(1u64)) // Mock block index
    })
}

#[query]
pub fn icrc1_balance_of(account: Account) -> Nat {
    STATE.with(|s| Nat::from(s.balances.get(&account).cloned().unwrap_or(0)))
}

#[query]
pub fn icrc1_decimals() -> u8 {
    8
}

#[query]
pub fn icrc1_symbol() -> String {
    "MOCK".to_string()
}

#[query]
pub fn icrc1_fee() -> Nat {
    Nat::from(10_000_u64)
}

#[update]
pub fn mint(account: Account, amount: u128) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let bal = s.balances.get(&account).cloned().unwrap_or(0);
        s.balances.insert(account, bal + amount);
    });
}
