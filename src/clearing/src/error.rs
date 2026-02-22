use candid::CandidType;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ClearingError {
    InsufficientExcessMargin,
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
}
