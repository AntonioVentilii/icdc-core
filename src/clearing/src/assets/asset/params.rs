use shared::types::Asset;

use crate::{assets::types::AssetAmount, types::account::LedgerAccount};

/// Parameters for retrieving an asset balance.
pub struct AssetBalanceOfParams<'a> {
    /// The asset to check the balance for.
    pub asset: &'a Asset,
    /// The account to check the balance of.
    pub account: LedgerAccount,
}

/// Parameters for a standard asset transfer.
pub struct AssetTransferParams<'a> {
    /// The asset to transfer.
    pub asset: &'a Asset,
    /// The source account.
    pub from: LedgerAccount,
    /// The destination account.
    pub to: LedgerAccount,
    /// The amount to transfer.
    pub amount: AssetAmount,
    /// Optional timestamp for ledger idempotency.
    pub created_at_time: Option<u64>,
}

/// Parameters for an asset transfer from a spender's allowance.
pub struct AssetTransferFromParams<'a> {
    /// The asset to transfer.
    pub asset: &'a Asset,
    /// The account with the allowance (typically the canister).
    pub spender: LedgerAccount,
    /// The source account (owner of the funds).
    pub from: LedgerAccount,
    /// The destination account.
    pub to: LedgerAccount,
    /// The amount to transfer.
    pub amount: AssetAmount,
    /// Optional timestamp for ledger idempotency.
    pub created_at_time: Option<u64>,
}
