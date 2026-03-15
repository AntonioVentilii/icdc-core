use candid::Principal;
use icrc_ledger_types::icrc1::account::Account;

#[must_use]
pub fn to_account(p: Principal) -> Account {
    Account {
        owner: p,
        subaccount: None,
    }
}
