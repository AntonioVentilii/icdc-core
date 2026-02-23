use candid::CandidType;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CommonError {
    Unauthorized,
    RegistryNotSet,
    Internal(String),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum LedgerError {
    TransferFailed(String),
    UnsupportedLedger,
    FetchingFeeFailed(String),
    FetchingBalanceFailed(String),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DepositCollateralError {
    Ledger(LedgerError),
    DepositCollateralMathOverflow,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawCollateralError {
    Ledger(LedgerError),
    InsufficientExcessMargin {
        current: candid::Nat,
        requested: candid::Nat,
        required: candid::Nat,
    },
    WithdrawCollateralMathOverflow,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MarginAccountError {
    Ledger(LedgerError),
    NoMarginAccountFound,
    BalanceMathOverflow,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum TradeError {
    Common(CommonError),
    SeriesNotFound,
    BuyerInsufficientMargin,
    SellerInsufficientMargin,
    GettingRegistrySeriesFailed(String),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SettlementError {
    Common(CommonError),
    Ledger(LedgerError),
    UnsupportedSettlementAsset,
    PayoffMathOverflow,
    FeeMathOverflow,
}
