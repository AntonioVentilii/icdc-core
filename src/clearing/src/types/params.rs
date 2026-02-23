use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{Asset, SeriesId};

use crate::types::{
    trade::{TradeId, TransferId},
    user::{DepositId, User, WithdrawalId},
};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositCollateralParams {
    pub amount: candid::Nat,
    pub asset: Asset,
    pub deposit_id: DepositId,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawCollateralParams {
    pub amount: candid::Nat,
    pub asset: Asset,
    pub withdrawal_id: WithdrawalId,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitMatchedTradeParams {
    pub trade_id: TradeId,
    pub series_id: SeriesId,
    pub buyer: User,
    pub seller: User,
    pub qty: i128,
    pub price: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetMarginAccountParams {
    pub refresh: Option<bool>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetPositionParams {
    pub series_id: SeriesId,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettleSeriesParams {
    pub series_id: SeriesId,
    pub settlement_price: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FreezePositionForTransferParams {
    pub transfer_id: TransferId,
    pub user: User,
    pub series_id: SeriesId,
}
