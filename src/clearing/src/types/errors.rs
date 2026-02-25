use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::SeriesId;

use crate::types::user::User;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CommonError {
    Unauthorized,
    RegistryNotSet,
    Internal(String),
    MathOverflow,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum LedgerError {
    TransferError(String),
    UnsupportedLedger,
    CallError {
        method: String,
        code: i32,
        message: String,
    },
    InsufficientBalance {
        balance: u128,
        required: u128,
    },
    MathOverflow,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DepositCollateralError {
    Ledger(LedgerError),
    MathOverflow,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawCollateralError {
    Ledger(LedgerError),
    InsufficientExcessMargin { current: u128, requested: u128 },
    MathOverflow,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MarginAccountError {
    Ledger(LedgerError),
    NoMarginAccountFound,
    MathOverflow,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum TradeError {
    Common(CommonError),
    SeriesNotFound(SeriesId),
    InsufficientMargin {
        user: User,
        balance: u128,
        required: u128,
    },
    RegistryError(String),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SettlementError {
    Common(CommonError),
    Ledger(LedgerError),
    UnsupportedSettlementAsset,
    MathOverflow,
}
