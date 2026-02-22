use candid::{CandidType, Deserialize, Principal};
use serde::Serialize;
use shared::types::Asset;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositCollateralParams {
    pub amount: candid::Nat,
    pub asset: Asset,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawCollateralParams {
    pub amount: candid::Nat,
    pub asset: Asset,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitMatchedTradeParams {
    pub series_id: String,
    pub buyer: Principal,
    pub seller: Principal,
    pub qty: i128,
    pub price: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetPositionParams {
    pub user: Principal,
    pub series_id: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettleSeriesParams {
    pub series_id: String,
    pub settlement_price: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FreezePositionForTransferParams {
    pub user: Principal,
    pub series_id: String,
}
