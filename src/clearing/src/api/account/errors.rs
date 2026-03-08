use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::asset::errors::AssetError;

/// Errors related to margin account retrieval or state.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MarginAccountError {
    /// An error occurred while interacting with the asset.
    Asset(AssetError),
    /// No margin account exists for the specified user and asset.
    NoMarginAccountFound,
    /// Overflow during account state calculation.
    MathOverflow,
}
