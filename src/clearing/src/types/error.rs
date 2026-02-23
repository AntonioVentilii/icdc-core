use candid::CandidType;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ClearingError {
    InsufficientExcessMargin {
        current: candid::Nat,
        requested: candid::Nat,
        required: candid::Nat,
    },
    NoMarginAccountFound,
    BuyerInsufficientMargin,
    SellerInsufficientMargin,
    TransferFailed(String),
    UnsupportedLedger,
    Unauthorized,
    RegistryNotSet,
    SeriesNotFound,
    UnsupportedSettlementAsset,
    GettingRegistrySeriesFailed(String),
    DepositCollateralMathOverflow,
    WithdrawCollateralMathOverflow,
    PayoffMathOverflow,
    FeeMathOverflow,
    BalanceMathOverflow,
    FetchingFeeFailed(String),
    FetchingBalanceFailed(String),
}
