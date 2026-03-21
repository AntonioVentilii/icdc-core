use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::types::errors::CommonError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawFundError {
    Common(CommonError),
    InsufficientFunds,
    TransferFailed(String),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CancelFundWithdrawalError {
    Common(CommonError),
    PlanNotFound,
    InvalidPlanStatus,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateAssetPriceError {
    Common(CommonError),
    AssetNotFound,
    OracleNotConfigured,
    AssetMetricsNotInitialized,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RegisterIcrcAssetError {
    Common(CommonError),
    AssetAlreadyExists,
    VusdCannotBeCollateral,
    /// `allowed_balance_domains` was empty or could not be normalized.
    InvalidAllowedBalanceDomains,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateCollateralAllowedDomainsError {
    AssetNotFound,
    InvalidAllowedBalanceDomains,
}
