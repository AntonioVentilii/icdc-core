use candid::Principal;
use sha2::{Digest, Sha256};
use shared::types::SettlementAsset;

use crate::memory::{ckusdc_ledger, icp_ledger};

/// Returns the ledger canister principal for a given [`SettlementAsset`].
pub fn get_ledger_for_asset(asset: &SettlementAsset) -> Principal {
    match asset {
        SettlementAsset::Icp => icp_ledger(),
        SettlementAsset::CkUsdc => ckusdc_ledger(),
    }
}

/// Derives the subaccount for a user in the current canister.
pub fn derive_user_subaccount(user: Principal) -> [u8; 32] {
    derive_user_subaccount_for_canister(ic_cdk::id(), user)
}

/// Derives a consistent subaccount for a user in a target canister using a salt.
pub fn derive_user_subaccount_for_canister(canister_id: Principal, user: Principal) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"collateral");
    hasher.update(canister_id.as_slice());
    hasher.update(user.as_slice());
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subaccount_derivation() {
        let canister_id = Principal::from_slice(&[0u8; 29]);
        let user = Principal::from_slice(&[1u8; 29]);
        let sub1 = derive_user_subaccount_for_canister(canister_id, user);
        let sub2 = derive_user_subaccount_for_canister(canister_id, user);
        assert_eq!(sub1, sub2);

        let user2 = Principal::from_slice(&[2u8; 29]);
        let sub3 = derive_user_subaccount_for_canister(canister_id, user2);
        assert_ne!(sub1, sub3);

        let canister_id2 = Principal::from_slice(&[3u8; 29]);
        let sub4 = derive_user_subaccount_for_canister(canister_id2, user);
        assert_ne!(sub1, sub4);
    }
}
