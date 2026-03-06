use candid::{CandidType, Deserialize, Nat};
use serde::Serialize;
use shared::types::Asset;

use crate::types::user::{DepositId, WithdrawalId};

/// Input parameters for depositing collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositCollateralParams {
    /// The amount of the asset to deposit.
    pub amount: Nat,
    /// The asset to be deposited.
    pub asset: Asset,
    /// Unique identifier for the deposit operation.
    pub deposit_id: DepositId,
}

/// Input parameters for withdrawing collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawCollateralParams {
    /// The amount of the asset to withdraw.
    pub amount: Nat,
    /// The asset to be withdrawn.
    pub asset: Asset,
    /// Unique identifier for the withdrawal operation.
    pub withdrawal_id: WithdrawalId,
}

/// Input parameters for blocking (reserving) collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BlockCollateralParams {
    /// The amount of the asset to block.
    pub amount: Nat,
    /// The asset to be blocked.
    pub asset: Asset,
}

/// Input parameters for unblocking (releasing) collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UnblockCollateralParams {
    /// The amount of the asset to unblock.
    pub amount: Nat,
    /// The asset to be unblocked.
    pub asset: Asset,
}
