use candid::Principal;

use crate::types::user::User;

/// Represents a ledger account within the system.
pub enum LedgerAccount {
    /// A user's internal clearing account.
    UserClearing(User),
    /// The canister's own main account.
    CanisterMain,
    /// An external account identified by principal and optional subaccount.
    External(Principal, Option<[u8; 32]>),
}
impl LedgerAccount {
    /// Returns the [`Principal`] associated with this account.
    pub fn principal(&self) -> Principal {
        match self {
            LedgerAccount::UserClearing(u) => u.principal(),
            LedgerAccount::CanisterMain => ic_cdk::id(),
            LedgerAccount::External(p, _) => *p,
        }
    }
}
