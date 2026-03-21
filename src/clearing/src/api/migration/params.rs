use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::BalanceDomain;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MigrateDomainParams {
    pub migration_id: String,
    pub from_domain: BalanceDomain,
    pub to_domain: BalanceDomain,
}
