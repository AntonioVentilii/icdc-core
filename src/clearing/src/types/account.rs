use candid::Principal;

use crate::types::user::User;

pub enum LedgerAccount {
    UserClearing(User),
    CanisterMain,
    External(Principal, Option<[u8; 32]>),
}

impl LedgerAccount {
    pub fn principal(&self) -> Principal {
        match self {
            LedgerAccount::UserClearing(u) => u.principal(),
            LedgerAccount::CanisterMain => ic_cdk::id(),
            LedgerAccount::External(p, _) => *p,
        }
    }
}
