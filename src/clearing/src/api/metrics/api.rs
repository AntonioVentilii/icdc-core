use core::cell::RefCell;
use std::collections::BTreeMap;

use candid::Nat;
use ic_cdk::{api::is_controller, caller};
use ic_cdk_macros::query;
use serde_bytes::ByteBuf;
use shared::types::{Asset, AssetId, CollateralAssetConfig};

use crate::{
    guards::caller_is_controller,
    memory::{ACCOUNT_STATES, COLLATERAL_ASSETS, EVENTS, POSITIONS, SERIES},
    types::{
        event::EventType,
        http::{HeaderField, HttpRequest, HttpResponse},
        margin::AccountState,
        stats::Stats,
        user::User,
    },
};

/// Exports internal state as a structured `Stats` object.
///
/// This method is gated to canister controllers.
#[query(guard = "caller_is_controller")]
#[must_use]
pub fn stats() -> Stats {
    // --- POSITIONS & OPEN INTEREST ---
    let (total_open_interest, total_reserved_margin) = POSITIONS.with(|p| {
        let p = p.borrow();
        let oi: u128 = p.values().map(|v| v.net_qty.unsigned_abs()).sum();
        let reserved: u128 = p.values().map(|v| v.reserved_margin_usd).sum();
        (oi, reserved)
    });

    // --- ACCOUNT STATES & BALANCES ---
    let (total_users, asset_balances_id) =
        ACCOUNT_STATES.with(|m: &RefCell<BTreeMap<User, AccountState>>| {
            let m = m.borrow();
            let mut balances: BTreeMap<AssetId, u128> = BTreeMap::new();
            for account in m.values() {
                for domain_balances in account.balances.values() {
                    for (asset_id, amount) in domain_balances {
                        *balances.entry(asset_id.clone()).or_insert(0_u128) += amount;
                    }
                }
            }
            (m.len() as u64, balances)
        });

    // --- Map AssetId back to Asset for Stats reporting ---
    let asset_balances = COLLATERAL_ASSETS.with(
        |configs: &RefCell<BTreeMap<AssetId, CollateralAssetConfig>>| {
            let configs = configs.borrow();
            let mut mapped = BTreeMap::new();
            for (id, amount) in asset_balances_id {
                if let Some(config) = configs.get(&id) {
                    mapped.insert(config.asset.clone(), amount);
                }
            }
            mapped
        },
    );

    // --- SERIES ---
    let total_series = SERIES.with(|s| s.borrow().len() as u64);

    // --- EVENTS & TRADES ---
    let (event_counts, trade_count) = EVENTS.with(|e| {
        let e = e.borrow();
        let mut counts = BTreeMap::new();
        let mut trades = 0;
        for event in e.iter() {
            let label = match event.event_type {
                EventType::OrderPlaced => "order_placed",
                EventType::Executed => {
                    trades += 1;
                    "executed"
                }
                EventType::Settled => "settled",
                EventType::Liquidated => "liquidated",
            };
            *counts.entry(label.to_owned()).or_insert(0_u64) += 1;
        }
        (counts, trades)
    });

    let margin_balances_nat = asset_balances
        .into_iter()
        .map(|(k, v)| (k, Nat::from(v)))
        .collect();

    Stats {
        open_interest: Nat::from(total_open_interest),
        total_collateral_locked: Nat::from(total_reserved_margin),
        total_users,
        total_series,
        total_trades: trade_count,
        margin_balances: margin_balances_nat,
        event_counts,
    }
}

/// Exports internal state as Prometheus metrics.
///
/// This method is gated to canister controllers.
#[query(guard = "caller_is_controller")]
#[must_use]
pub fn metrics() -> String {
    let stats = stats();

    let mut metrics = String::new();

    metrics.push_str(
        "# HELP clearing_open_interest Total absolute open interest across all series.\n",
    );
    metrics.push_str("# TYPE clearing_open_interest gauge\n");
    metrics.push_str(&format!("clearing_open_interest {}\n", stats.open_interest));

    metrics.push_str(
        "# HELP clearing_total_collateral_locked Total collateral locked in positions.\n",
    );
    metrics.push_str("# TYPE clearing_total_collateral_locked gauge\n");
    metrics.push_str(&format!(
        "clearing_total_collateral_locked {}\n",
        stats.total_collateral_locked
    ));

    metrics.push_str("# HELP clearing_users_total Total number of unique margin accounts.\n");
    metrics.push_str("# TYPE clearing_users_total gauge\n");
    metrics.push_str(&format!("clearing_users_total {}\n", stats.total_users));

    metrics.push_str("# HELP clearing_total_margin_balance Total collateral balance per asset in margin accounts.\n");
    metrics.push_str("# TYPE clearing_total_margin_balance gauge\n");
    for (asset, balance) in stats.margin_balances {
        let asset_str = match asset {
            Asset::Icrc(p) => p.to_text(),
            Asset::NativeEvm(asset) => asset.to_string(),
            Asset::Erc20(token) => token.to_string(),
        };
        metrics.push_str(&format!(
            "clearing_total_margin_balance{{asset=\"{asset_str}\"}} {balance}\n"
        ));
    }

    metrics.push_str("# HELP clearing_series_total Total number of derivative series.\n");
    metrics.push_str("# TYPE clearing_series_total gauge\n");
    metrics.push_str(&format!("clearing_series_total {}\n", stats.total_series));

    metrics.push_str("# HELP clearing_trade_count_total Total number of executed trades.\n");
    metrics.push_str("# TYPE clearing_trade_count_total counter\n");
    metrics.push_str(&format!(
        "clearing_trade_count_total {}\n",
        stats.total_trades
    ));

    metrics.push_str("# HELP clearing_event_count_total Total number of events by type.\n");
    metrics.push_str("# TYPE clearing_event_count_total counter\n");
    for (label, count) in stats.event_counts {
        metrics.push_str(&format!(
            "clearing_event_count_total{{type=\"{label}\"}} {count}\n"
        ));
    }

    metrics
}

#[query]
#[must_use]
#[expect(clippy::needless_pass_by_value)]
pub fn http_request(req: HttpRequest) -> HttpResponse {
    if req.method != "GET" || req.url != "/metrics" {
        return HttpResponse {
            status_code: 404,
            headers: vec![],
            body: ByteBuf::from("Not Found"),
        };
    }

    if !is_controller(&caller()) {
        return HttpResponse {
            status_code: 403,
            headers: vec![],
            body: ByteBuf::from("Forbidden: Controller only"),
        };
    }

    let body = metrics();

    HttpResponse {
        status_code: 200,
        headers: vec![
            HeaderField(
                "Content-Type".to_owned(),
                "text/plain; version=0.0.4".to_owned(),
            ),
            HeaderField("Content-Length".to_owned(), body.len().to_string()),
        ],
        body: ByteBuf::from(body),
    }
}
