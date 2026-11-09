//! Timelocked actions (`docs/risk/margin-model.md`, section 12).
//!
//! A change that could lower margin, widen administrative power or bring new data into the model is
//! first proposed, stays publicly queryable for the timelock duration, and only then can be
//! executed.

use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::risk::fixed::Fixed;

/// Maximum number of actions waiting at the same time.
pub const MAX_PENDING_ACTIONS: usize = 256;

/// A change that waits behind the timelock.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TimelockedAction {
    /// Lower the manual multiplier of an underlying.
    LowerRiskMultiplier { underlying: String, k: Fixed },
    /// Lower the global multiplier.
    LowerGlobalRiskMultiplier { k: Fixed },
    /// Merge a sealed history upload into its underlying's history.
    ActivateHistoryUpload { upload_id: u64 },
    /// Grant the risk administrator role.
    AddRiskAdmin { principal: Principal },
    /// Shorten the timelock itself.
    LowerTimelockDuration { duration_ns: u64 },
}

/// An action waiting for its timelock to expire.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PendingAction {
    pub id: u64,
    pub action: TimelockedAction,
    pub proposed_by: Principal,
    pub proposed_at_ns: u64,
    pub executable_at_ns: u64,
}
