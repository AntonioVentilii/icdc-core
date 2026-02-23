use icrc_ledger_types::icrc1::account::{Account, Subaccount};

use crate::{types::user::User, utils::account::derive_user_subaccount};

pub trait ClearingAccountExt {
    fn clearing_subaccount(&self) -> Subaccount;
    fn clearing_account(&self) -> Account;
}
impl ClearingAccountExt for User {
    fn clearing_subaccount(&self) -> Subaccount {
        derive_user_subaccount(self.0)
    }

    fn clearing_account(&self) -> Account {
        // Delegate to custody_subaccount (single canonical derivation)
        Account {
            owner: ic_cdk::id(),
            subaccount: Some(self.clearing_subaccount()),
        }
    }
}
