use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{AssetId, CollateralAssetConfig};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum FundType {
    Insurance,
    Treasury,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawFundParams {
    pub fund_type: FundType,
    pub asset_id: AssetId,
    pub amount: u128,
    pub to: Principal,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UpdateCollateralAssetParams {
    pub config: CollateralAssetConfig,
}
