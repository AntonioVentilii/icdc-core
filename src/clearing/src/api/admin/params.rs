use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{
    AssetId, AssetMetrics, BalanceDomain, CollateralAssetConfig, DomainPolicy, Price,
};

#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FundType {
    Insurance,
    Treasury,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawFundParams {
    pub request_id: String,
    pub fund_type: FundType,
    pub asset_id: AssetId,
    pub amount: u128,
    pub to: Principal,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UpdateCollateralAssetParams {
    pub config: CollateralAssetConfig,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UpdateAssetMetricsParams {
    pub asset_id: AssetId,
    pub metrics: AssetMetrics,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UpdateAssetPriceParams {
    pub asset_id: AssetId,
    pub price: Price,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CancelFundWithdrawalParams {
    pub request_id: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RegisterIcrcAssetParams {
    pub asset_id: AssetId,
    pub ledger_id: Principal,
    pub haircut_bps: u16,
    pub oracle_id: Option<String>,
    pub is_enabled: bool,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UpdateDomainPolicyParams {
    pub domain: BalanceDomain,
    pub policy: DomainPolicy,
}
