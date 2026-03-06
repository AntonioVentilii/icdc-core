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
