use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::types::errors::CommonError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawFundError {
    Common(CommonError),
    InsufficientFunds,
    TransferFailed(String),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CancelFundWithdrawalError {
    Common(CommonError),
    PlanNotFound,
    InvalidPlanStatus,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateAssetPriceError {
    Common(CommonError),
    AssetNotFound,
    OracleNotConfigured,
    AssetMetricsNotInitialized,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RegisterIcrcAssetError {
    Common(CommonError),
    AssetAlreadyExists,
    VusdCannotBeCollateral,
    /// `allowed_balance_domains` was empty or could not be normalized.
    InvalidAllowedBalanceDomains,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateCollateralAllowedDomainsError {
    AssetNotFound,
    InvalidAllowedBalanceDomains,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RefreshIcrcAssetMetadataError {
    Common(CommonError),
    AssetNotFound,
    NotAnIcrcAsset,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ReassignAccountError {
    Common(CommonError),
    /// `old_owner` and `new_owner` are the same principal.
    SameOwner,
    /// `old_owner` has no clearing account to reassign.
    AccountNotFound,
    /// `new_owner` already has clearing state; this primitive reassigns, it never merges.
    TargetAccountNotEmpty,
    /// `old_owner` has resting limit orders; they must be cancelled first so the
    /// book's ownership assumptions are never mutated behind its back.
    OpenOrdersExist,
    /// `old_owner` or `new_owner` has non-finalised deposit, withdrawal, settlement,
    /// or domain-migration plans that would act on the wrong owner mid-flight.
    InFlightPlansExist,
    /// `old_owner` has positions frozen for cross-canister transfer; the signed
    /// `PositionProof`s are bound to the old principal and cannot be reassigned.
    PendingPositionTransfersExist,
}
