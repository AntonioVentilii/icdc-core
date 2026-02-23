use shared::types::Asset;

use crate::memory::{ckusdc_ledger, icp_ledger};

pub fn is_supported_asset(asset: &Asset) -> bool {
    match asset {
        Asset::Icrc(ledger_id) => ledger_id == &icp_ledger() || ledger_id == &ckusdc_ledger(),
    }
}
