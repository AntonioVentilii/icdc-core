use candid::Principal;

use crate::types::user::User;

/// Represents an externally controlled account.
pub enum ExternalAssetAccount {
    /// A simple principal.
    Principal(Principal),
    /// An ICP/ICRC-style account.
    Icrc {
        owner: Principal,
        subaccount: Option<[u8; 32]>,
    },
    /// An EVM address.
    Evm(String),
}

/// Represents an asset account within the system.
pub enum AssetAccount {
    /// A user's internal clearing account.
    UserClearing(User),
    /// The canister's own main account.
    CanisterMain,
    /// An external account.
    External(ExternalAssetAccount),
}
impl AssetAccount {
    /// Creates an `AssetAccount` for a simple principal.
    pub fn external_principal(principal: Principal) -> Self {
        Self::External(ExternalAssetAccount::Principal(principal))
    }

    /// Creates an `AssetAccount` for an ICP/ICRC-style account.
    pub fn external_icrc(owner: Principal, subaccount: Option<[u8; 32]>) -> Self {
        Self::External(ExternalAssetAccount::Icrc { owner, subaccount })
    }

    /// Creates an `AssetAccount` for an EVM address.
    #[expect(dead_code)]
    pub fn external_evm(address: impl Into<String>) -> Self {
        Self::External(ExternalAssetAccount::Evm(address.into()))
    }
}
