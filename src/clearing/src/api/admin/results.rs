use std::collections::BTreeMap;

use candid::{CandidType, Nat};
use serde::{Deserialize, Serialize};
use shared::{impl_from_result, types::AssetId};

use super::errors::{
    CancelFundWithdrawalError, ReassignAccountError, RefreshIcrcAssetMetadataError,
    RegisterIcrcAssetError, UpdateAssetPriceError, UpdateCollateralAllowedDomainsError,
    WithdrawFundError,
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
impl_from_result!(WithdrawFundResult, Nat, WithdrawFundError);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CancelFundWithdrawalResult {
    Ok,
    Err(CancelFundWithdrawalError),
}
impl_from_result!(CancelFundWithdrawalResult, (), CancelFundWithdrawalError);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateAssetPriceResult {
    Ok,
    Err(UpdateAssetPriceError),
}
impl_from_result!(UpdateAssetPriceResult, (), UpdateAssetPriceError);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RegisterIcrcAssetResult {
    Ok,
    Err(RegisterIcrcAssetError),
}
impl_from_result!(RegisterIcrcAssetResult, (), RegisterIcrcAssetError);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateCollateralAllowedDomainsResult {
    Ok,
    Err(UpdateCollateralAllowedDomainsError),
}
impl_from_result!(
    UpdateCollateralAllowedDomainsResult,
    (),
    UpdateCollateralAllowedDomainsError
);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RefreshIcrcAssetMetadataResult {
    Ok,
    Err(RefreshIcrcAssetMetadataError),
}
impl_from_result!(
    RefreshIcrcAssetMetadataResult,
    (),
    RefreshIcrcAssetMetadataError
);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ReassignAccountResult {
    Ok,
    Err(ReassignAccountError),
}
impl_from_result!(ReassignAccountResult, (), ReassignAccountError);
