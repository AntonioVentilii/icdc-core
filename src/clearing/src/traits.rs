use ic_cdk::api::canister_self;
use icrc_ledger_types::icrc1::account::{Account, Subaccount};

use crate::{types::user::User, utils::account::derive_user_subaccount};

/// Extension trait for [`User`] to provide clearing-specific account derivation.
pub trait ClearingAccountExt {
    /// Derives the unique subaccount for this user within the clearing canister.
    fn clearing_subaccount(&self) -> Subaccount;
    /// Returns the full ICRC [`Account`] (canister principal + user subaccount) for this user in
    /// the clearing canister.
    fn clearing_account(&self) -> Account;
}
impl ClearingAccountExt for User {
    fn clearing_subaccount(&self) -> Subaccount {
        derive_user_subaccount(self.0)
    }

    fn clearing_account(&self) -> Account {
        // Delegate to custody_subaccount (single canonical derivation)
        Account {
            owner: canister_self(),
            subaccount: Some(self.clearing_subaccount()),
        }
    }
}
