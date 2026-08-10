use shared::types::{Asset, AssetId};

use crate::{assets::types::AssetAmount, types::account::AssetAccount};

/// Parameters for retrieving an asset balance.
pub struct AssetBalanceOfParams<'a> {
    /// The asset to check the balance for.
    pub asset: &'a Asset,
    /// The account to check the balance of.
    pub account: AssetAccount,
}

/// Parameters for a standard asset transfer.
pub struct AssetTransferParams<'a> {
    /// The asset to transfer.
    pub asset: &'a Asset,
    /// The identifier of the asset, used to read and self-heal the cached
    /// transfer fee (see [`crate::assets::icrc::IcrcHandler::transfer`]).
    pub asset_id: &'a AssetId,
    /// The source account.
    pub from: AssetAccount,
    /// The destination account.
    pub to: AssetAccount,
    /// The amount to transfer.
    pub amount: AssetAmount,
    /// Optional timestamp in nanoseconds for ledger idempotency.
    pub created_at_time_ns: Option<u64>,
}

/// Parameters for an asset transfer from a spender's allowance.
pub struct AssetTransferFromParams<'a> {
    /// The asset to transfer.
    pub asset: &'a Asset,
    /// The account with the allowance (typically the canister).
    pub spender: AssetAccount,
    /// The source account (owner of the funds).
    pub from: AssetAccount,
    /// The destination account.
    pub to: AssetAccount,
    /// The amount to transfer.
    pub amount: AssetAmount,
    /// Optional timestamp in nanoseconds for ledger idempotency.
    pub created_at_time_ns: Option<u64>,
}
