use candid::{CandidType, Deserialize, Nat};
use serde::Serialize;
use shared::types::{AssetId, BalanceDomain};

use crate::types::user::{DepositId, WithdrawalId};

/// Input parameters for depositing collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositCollateralParams {
    /// The amount of the asset to deposit.
    pub amount: Nat,
    /// The unique identifier of the asset to deposit.
    pub asset_id: AssetId,
    /// Unique identifier for the deposit operation.
    pub deposit_id: DepositId,
    /// The specific balance domain to deposit into (defaults to Settlement).
    pub domain: Option<BalanceDomain>,
}

/// Input parameters for withdrawing collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawCollateralParams {
    /// The amount of the asset to withdraw.
    pub amount: Nat,
    /// The unique identifier of the asset to withdraw.
    pub asset_id: AssetId,
    /// Unique identifier for the withdrawal operation.
    pub withdrawal_id: WithdrawalId,
    /// The specific balance domain to withdraw from (defaults to Settlement).
    pub domain: Option<BalanceDomain>,
}

/// Input parameters for blocking (reserving) collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BlockCollateralParams {
    /// The amount of the asset to block.
    pub amount: Nat,
    /// The unique identifier of the asset to block.
    pub asset_id: AssetId,
}

/// Input parameters for unblocking (releasing) collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UnblockCollateralParams {
    /// The amount of the asset to unblock.
    pub amount: Nat,
    /// The unique identifier of the asset to unblock.
    pub asset_id: AssetId,
}
