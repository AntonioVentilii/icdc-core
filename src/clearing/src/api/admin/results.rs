use std::collections::BTreeMap;

use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::AssetId;

use crate::types::errors::CommonError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetFundsResult {
    pub insurance_fund: BTreeMap<AssetId, u128>,
    pub treasury: BTreeMap<AssetId, u128>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawFundError {
    Common(CommonError),
    InsufficientFunds,
    TransferFailed(String),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawFundResult {
    Ok(candid::Nat),
    Err(WithdrawFundError),
}
impl From<Result<candid::Nat, WithdrawFundError>> for WithdrawFundResult {
    fn from(res: Result<candid::Nat, WithdrawFundError>) -> Self {
        match res {
            Ok(v) => Self::Ok(v),
            Err(e) => Self::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CancelFundWithdrawalError {
    Common(CommonError),
    PlanNotFound,
    InvalidPlanStatus,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CancelFundWithdrawalResult {
    Ok,
    Err(CancelFundWithdrawalError),
}
impl From<Result<(), CancelFundWithdrawalError>> for CancelFundWithdrawalResult {
    fn from(res: Result<(), CancelFundWithdrawalError>) -> Self {
        match res {
            Ok(_) => Self::Ok,
            Err(e) => Self::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateAssetPriceError {
    Common(CommonError),
    AssetNotFound,
    OracleNotConfigured,
    AssetMetricsNotInitialized,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateAssetPriceResult {
    Ok,
    Err(UpdateAssetPriceError),
}

impl From<Result<(), UpdateAssetPriceError>> for UpdateAssetPriceResult {
    fn from(res: Result<(), UpdateAssetPriceError>) -> Self {
        match res {
            Ok(_) => Self::Ok,
            Err(e) => Self::Err(e),
        }
    }
}
