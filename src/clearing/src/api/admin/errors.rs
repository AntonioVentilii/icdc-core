use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{asset::errors::AssetError, AssetId};

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
    /// One of the two principals is anonymous. Every account, position, deposit,
    /// and withdrawal API rejects the anonymous caller, so an account assigned to
    /// it would be permanently unreachable.
    AnonymousOwner,
    /// Another reassignment touching one of the two principals has not finalised.
    /// Replay that one's `reassignment_id` to resume it.
    ReassignmentInProgress,
    /// The `reassignment_id` is already in use for `old_owner` with a different
    /// `new_owner`.
    ReassignmentIdReused,
    /// The account holds an asset whose custody this canister cannot move
    /// on-chain, so the funds would be stranded under the old principal.
    UnsupportedCustodyAsset {
        asset_id: AssetId,
    },
    /// A custody sweep did not settle. The re-key already happened and the plan
    /// is still pending: replay the same `reassignment_id` to resume the sweep.
    CustodyTransferFailed {
        asset_id: AssetId,
        error: AssetError,
    },
}
