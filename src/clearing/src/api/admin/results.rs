use std::collections::BTreeMap;

use candid::{CandidType, Nat};
use serde::{Deserialize, Serialize};
use shared::types::AssetId;

use super::errors::{
    CancelFundWithdrawalError, RegisterIcrcAssetError, UpdateAssetPriceError,
    UpdateCollateralAllowedDomainsError, WithdrawFundError,
};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetFundsResult {
    pub insurance_fund: BTreeMap<AssetId, u128>,
    pub treasury: BTreeMap<AssetId, u128>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawFundResult {
    Ok(Nat),
    Err(WithdrawFundError),
}
impl From<Result<Nat, WithdrawFundError>> for WithdrawFundResult {
    fn from(res: Result<Nat, WithdrawFundError>) -> Self {
        match res {
            Ok(v) => Self::Ok(v),
            Err(e) => Self::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CancelFundWithdrawalResult {
    Ok,
    Err(CancelFundWithdrawalError),
}
impl From<Result<(), CancelFundWithdrawalError>> for CancelFundWithdrawalResult {
    fn from(res: Result<(), CancelFundWithdrawalError>) -> Self {
        match res {
            Ok(()) => Self::Ok,
            Err(e) => Self::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateAssetPriceResult {
    Ok,
    Err(UpdateAssetPriceError),
}
impl From<Result<(), UpdateAssetPriceError>> for UpdateAssetPriceResult {
    fn from(res: Result<(), UpdateAssetPriceError>) -> Self {
        match res {
            Ok(()) => Self::Ok,
            Err(e) => Self::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RegisterIcrcAssetResult {
    Ok,
    Err(RegisterIcrcAssetError),
}
impl From<Result<(), RegisterIcrcAssetError>> for RegisterIcrcAssetResult {
    fn from(res: Result<(), RegisterIcrcAssetError>) -> Self {
        match res {
            Ok(()) => Self::Ok,
            Err(e) => Self::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateCollateralAllowedDomainsResult {
    Ok,
    Err(UpdateCollateralAllowedDomainsError),
}
impl From<Result<(), UpdateCollateralAllowedDomainsError>>
    for UpdateCollateralAllowedDomainsResult
{
    fn from(res: Result<(), UpdateCollateralAllowedDomainsError>) -> Self {
        match res {
            Ok(()) => Self::Ok,
            Err(e) => Self::Err(e),
        }
    }
}
