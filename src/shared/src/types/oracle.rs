use std::collections::BTreeSet;

use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::types::description::Description;

/// Metadata about a price oracle entity.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OracleMetadata {
    /// The human-readable name of the oracle.
    pub name: String,
    /// Optional URL to the oracle's website.
    pub website: Option<String>,
    /// A short description of the oracle's methodology or data sources.
    pub description: Option<Description>,
}

/// Represents an authorised price oracle group.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Oracle {
    /// Unique identifier for the oracle (e.g., "COINGECKO").
    pub oracle_id: String,
    /// Metadata about the oracle.
    pub metadata: OracleMetadata,
    /// The set of principals authorised to push settlement data for this oracle.
    pub authorized_principals: BTreeSet<Principal>,
    /// The principal identifier of the oracle's manager (defaults to creator).
    pub manager: Principal,
    /// Timestamp of oracle registration in nanoseconds since UNIX epoch.
    pub registered_at_ns: u64,
}
