use shared::types::Asset;

use crate::{assets::types::AssetAmount, types::account::LedgerAccount};

pub struct AssetBalanceOfParams<'a> {
    pub asset: &'a Asset,
    pub account: LedgerAccount,
}

pub struct AssetTransferParams<'a> {
    pub asset: &'a Asset,
    pub from: LedgerAccount,
    pub to: LedgerAccount,
    pub amount: AssetAmount,
    pub created_at_time: Option<u64>,
}

pub struct AssetTransferFromParams<'a> {
    pub asset: &'a Asset,
    pub spender: LedgerAccount,
    pub from: LedgerAccount,
    pub to: LedgerAccount,
    pub amount: AssetAmount,
    pub created_at_time: Option<u64>,
}
