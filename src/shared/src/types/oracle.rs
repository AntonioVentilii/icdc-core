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

/// Input parameters for registering a new price oracle.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AddOracleParams {
    /// Unique identifier for the oracle (e.g., "COINGECKO").
    pub oracle_id: String,
    /// Initial information about the oracle.
    pub metadata: OracleMetadata,
    /// Initial list of authorised principals.
    pub authorized_principals: Vec<Principal>,
}

/// Input parameters for updating an existing oracle's metadata.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UpdateOracleMetadataParams {
    /// The unique identifier of the oracle to update.
    pub oracle_id: String,
    /// The updated metadata.
    pub metadata: OracleMetadata,
}

/// Input parameters for managing authorised principals of an oracle.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ManageOraclePrincipalsParams {
    /// The unique identifier of the oracle.
    pub oracle_id: String,
    /// Principals to be added to the authorised list.
    pub add_principals: Vec<Principal>,
    /// Principals to be removed from the authorised list.
    pub remove_principals: Vec<Principal>,
}

/// Errors that can occur during oracle-related operations.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum OracleError {
    /// Returned when attempting to add an oracle that already exists.
    OracleAlreadyExists,
    /// Returned when the specified oracle does not exist.
    OracleNotFound,
    /// Returned when the caller is not authorised to manage the oracle.
    UnauthorizedOracleManager,
}

/// The result of an oracle-related operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum OracleResult {
    Ok,
    Err(OracleError),
}

impl From<Result<(), OracleError>> for OracleResult {
    fn from(value: Result<(), OracleError>) -> Self {
        match value {
            Ok(()) => OracleResult::Ok,
            Err(e) => OracleResult::Err(e),
        }
    }
}
