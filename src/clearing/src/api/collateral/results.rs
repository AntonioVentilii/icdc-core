use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::impl_from_result;

use super::errors::{BlockingError, DepositCollateralError, WithdrawCollateralError};

/// Outcome of a collateral blocking operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum BlockCollateralResult {
    /// Collateral was successfully blocked.
    Ok,
    /// Failed to block collateral.
    Err(BlockingError),
}
impl_from_result!(BlockCollateralResult, (), BlockingError);

/// Outcome of a collateral unblocking operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UnblockCollateralResult {
    /// Collateral was successfully unblocked.
    Ok,
    /// Failed to unblock collateral.
    Err(BlockingError),
}
impl_from_result!(UnblockCollateralResult, (), BlockingError);

/// Outcome of a collateral deposit operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DepositCollateralResult {
    /// Deposit was successfully planned or executed.
    Ok,
    /// Failed to process deposit.
    Err(DepositCollateralError),
}
impl_from_result!(DepositCollateralResult, (), DepositCollateralError);

/// Outcome of a collateral withdrawal operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawCollateralResult {
    /// Withdrawal was successfully planned or executed.
    Ok,
    /// Failed to process withdrawal.
    Err(WithdrawCollateralError),
}
impl_from_result!(WithdrawCollateralResult, (), WithdrawCollateralError);
