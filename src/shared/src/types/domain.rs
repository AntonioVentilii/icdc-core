use std::collections::BTreeSet;

use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Represents a distinct domain for balances and trading logic.
#[derive(
    CandidType,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
)]
pub enum BalanceDomain {
    /// Testnet / sandbox collateral (e.g. TESTICP, Sepolia) — not real funds.
    Playground,
    /// Real collateralized trading (e.g., using ckUSDC, ICP).
    #[default]
    Settlement,
    /// VICI XP (loyalty points) — segregated from Playground test assets.
    ViciXp,
    /// Non-monetary social bets (e.g., betting a pizza).
    Social,
}

/// Building an [`AllowedBalanceDomains`] failed (e.g. empty input).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllowedBalanceDomainsError {
    /// At least one domain is required.
    Empty,
}

/// Non-empty allowlist of balance domains for collateral movement.
///
/// Built via [`TryFrom<Vec<BalanceDomain>>`]: duplicates are removed and order is stable
/// (`BalanceDomain` sort order).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedBalanceDomains(Vec<BalanceDomain>);

impl Default for AllowedBalanceDomains {
    fn default() -> Self {
        Self(vec![BalanceDomain::Settlement, BalanceDomain::Playground])
    }
}

impl TryFrom<Vec<BalanceDomain>> for AllowedBalanceDomains {
    type Error = AllowedBalanceDomainsError;

    fn try_from(domains: Vec<BalanceDomain>) -> Result<Self, Self::Error> {
        if domains.is_empty() {
            return Err(AllowedBalanceDomainsError::Empty);
        }
        let set: BTreeSet<BalanceDomain> = domains.into_iter().collect();
        Ok(Self(set.into_iter().collect()))
    }
}

impl From<AllowedBalanceDomains> for Vec<BalanceDomain> {
    fn from(value: AllowedBalanceDomains) -> Self {
        value.0
    }
}

impl AsRef<[BalanceDomain]> for AllowedBalanceDomains {
    fn as_ref(&self) -> &[BalanceDomain] {
        &self.0
    }
}

impl AllowedBalanceDomains {
    #[must_use]
    pub fn as_slice(&self) -> &[BalanceDomain] {
        &self.0
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<BalanceDomain> {
        self.0
    }
}

/// Configurable policy for a balance domain.
///
/// Controls domain-specific behavior such as fee ratios and feature flags.
/// Domains default to identical policies so risk enforcement is uniform unless overridden.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DomainPolicy {
    /// Whether deposits of real collateral assets are accepted in this domain.
    pub deposits_enabled: bool,
    /// Whether withdrawals of real collateral assets are allowed from this domain.
    pub withdrawals_enabled: bool,
    /// Optional override for the insurance fund fee ratio (bps).
    /// When `None`, the global `Config` value is used.
    pub insurance_fund_fee_ratio_override: Option<u16>,
    /// Optional override for the protocol fee ratio (bps).
    /// When `None`, the global `Config` value is used.
    pub protocol_fee_ratio_override: Option<u16>,
    /// Human-readable label for this domain.
    pub label: String,
}

impl Default for DomainPolicy {
    fn default() -> Self {
        Self {
            deposits_enabled: true,
            withdrawals_enabled: true,
            insurance_fund_fee_ratio_override: None,
            protocol_fee_ratio_override: None,
            label: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_balance_domains_try_from_empty_err() {
        assert_eq!(
            AllowedBalanceDomains::try_from(Vec::new()),
            Err(AllowedBalanceDomainsError::Empty)
        );
    }

    #[test]
    fn allowed_balance_domains_try_from_dedupes_and_sorts() {
        let got = AllowedBalanceDomains::try_from(vec![
            BalanceDomain::Settlement,
            BalanceDomain::Playground,
            BalanceDomain::ViciXp,
            BalanceDomain::Settlement,
        ])
        .unwrap();
        assert_eq!(
            got.as_slice(),
            &[
                BalanceDomain::Playground,
                BalanceDomain::Settlement,
                BalanceDomain::ViciXp
            ]
        );
    }
}
