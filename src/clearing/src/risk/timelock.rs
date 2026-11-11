//! Timelocked actions (`docs/risk/margin-model.md`, section 12).
//!
//! A change that could lower margin, widen administrative power or bring new data into the model is
//! first proposed, stays publicly queryable for the timelock duration, and only then can be
//! executed. An action records the value it expects to change from, so that executing it after the
//! state has moved on (for example after an emergency raise) is rejected instead of silently
//! overriding the newer value.

use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::risk::fixed::Fixed;

/// Maximum number of actions proposed by risk administrators waiting at the same time.
pub const MAX_PENDING_ACTIONS: usize = 256;

/// Maximum number of actions proposed by controllers waiting at the same time. Counted separately
/// from [`MAX_PENDING_ACTIONS`], so risk administrators can never crowd controllers out.
pub const MAX_PENDING_CONTROLLER_ACTIONS: usize = 16;

/// A change that waits behind the timelock.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TimelockedAction {
    /// Lower the manual multiplier of an underlying from `from` to `to`.
    LowerRiskMultiplier {
        underlying: String,
        from: Fixed,
        to: Fixed,
    },
    /// Lower the global multiplier from `from` to `to`.
    LowerGlobalRiskMultiplier { from: Fixed, to: Fixed },
    /// Change the timelock duration from `from_ns` to `to_ns`: any shortening, or a lengthening
    /// beyond twice the current value.
    ChangeTimelockDuration { from_ns: u64, to_ns: u64 },
    /// Merge a sealed history upload into its underlying's history.
    ActivateHistoryUpload { upload_id: u64 },
    /// Remove the history points of `underlying` dated `from_date..=to_date`, provided those
    /// points still hash to `expected_sha256` (the canonical hash of the range when it was
    /// proposed).
    RemoveHistoryPoints {
        underlying: String,
        from_date: u32,
        to_date: u32,
        expected_sha256: String,
    },
    /// Grant the risk administrator role.
    AddRiskAdmin { principal: Principal },
}

/// What a pending action applies to. At most one action per key waits at a time: a new proposal
/// with the same key replaces the old one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionKey {
    RiskMultiplier(String),
    GlobalRiskMultiplier,
    TimelockDuration,
    HistoryUpload(u64),
    HistoryRemoval(String),
    RiskAdmin(Principal),
}

impl TimelockedAction {
    /// The key that identifies what this action changes.
    #[must_use]
    pub fn key(&self) -> ActionKey {
        match self {
            Self::LowerRiskMultiplier { underlying, .. } => {
                ActionKey::RiskMultiplier(underlying.clone())
            }
            Self::LowerGlobalRiskMultiplier { .. } => ActionKey::GlobalRiskMultiplier,
            Self::ChangeTimelockDuration { .. } => ActionKey::TimelockDuration,
            Self::ActivateHistoryUpload { upload_id } => ActionKey::HistoryUpload(*upload_id),
            Self::RemoveHistoryPoints { underlying, .. } => {
                ActionKey::HistoryRemoval(underlying.clone())
            }
            Self::AddRiskAdmin { principal } => ActionKey::RiskAdmin(*principal),
        }
    }

    /// Whether only controllers propose this action (counted against
    /// [`MAX_PENDING_CONTROLLER_ACTIONS`]).
    #[must_use]
    pub fn is_controller_action(&self) -> bool {
        matches!(self, Self::AddRiskAdmin { .. })
    }
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

#[cfg(test)]
mod tests {
    use candid::Principal;

    use super::{ActionKey, TimelockedAction};
    use crate::risk::fixed::Fixed;

    #[test]
    fn keys_separate_parameters_and_underlyings() {
        let lower = |underlying: &str| TimelockedAction::LowerRiskMultiplier {
            underlying: underlying.to_owned(),
            from: Fixed::from_int(2),
            to: Fixed::ONE,
        };
        assert_eq!(
            lower("BTC").key(),
            ActionKey::RiskMultiplier("BTC".to_owned())
        );
        assert_ne!(lower("BTC").key(), lower("ETH").key());
        assert_eq!(
            TimelockedAction::ChangeTimelockDuration {
                from_ns: 1,
                to_ns: 2
            }
            .key(),
            TimelockedAction::ChangeTimelockDuration {
                from_ns: 3,
                to_ns: 4
            }
            .key()
        );
        let admin = TimelockedAction::AddRiskAdmin {
            principal: Principal::from_slice(&[1]),
        };
        assert!(admin.is_controller_action());
        assert!(!lower("BTC").is_controller_action());
    }
}
