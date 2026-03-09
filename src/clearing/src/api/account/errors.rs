use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::asset::errors::AssetError;

/// Errors related to account state retrieval or state.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AccountStateError {
    /// An error occurred while interacting with the asset.
    Asset(AssetError),
    /// No account state exists for the specified user.
    NoAccountStateFound,
    /// Overflow during account state calculation.
    MathOverflow,
}
