use std::{cell::RefCell, collections::BTreeMap};

use candid::{CandidType, Deserialize, Principal};
use ic_cdk::{
    api::management_canister::http_request::{
        http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse,
        TransformArgs, TransformContext,
    },
    caller,
};
use ic_cdk_macros::{post_upgrade, pre_upgrade, query, update};
use serde_json::Value;
use shared::types::{price::Price, AssetId};

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct UpdateAssetPriceParams {
    pub asset_id: AssetId,
    pub price: Price,
}

thread_local! {
    /// Stores the latest prices for each asset.
    static PRICES: RefCell<BTreeMap<String, Price>> = RefCell::new(BTreeMap::new());

    /// Configuration for the pricer.
    static CONFIG: RefCell<PricerConfig> = RefCell::new(PricerConfig::default());

    /// Assets to track periodically.
    static ASSETS_TO_TRACK: RefCell<Vec<String>> = RefCell::new(vec!["icp".to_string(), "ckbtc".to_string(), "cketh".to_string()]);
}

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct PricerConfig {
    pub owner: Principal,
    pub registry_canister: Option<Principal>,
    pub clearing_canister: Option<Principal>,
}

impl Default for PricerConfig {
    fn default() -> Self {
        Self {
            owner: Principal::anonymous(),
            registry_canister: None,
            clearing_canister: None,
        }
    }
}

#[ic_cdk::init]
fn init(registry_canister: Option<Principal>) {
    CONFIG.with(|config| {
        let mut config = config.borrow_mut();
        config.owner = caller();
        config.registry_canister = registry_canister;
    });

    // Start the timer to fetch prices every 5 minutes
    ic_cdk_timers::set_timer_interval(std::time::Duration::from_secs(300), || {
        ic_cdk::spawn(fetch_all_prices());
    });
}

async fn fetch_all_prices() {
    let assets = ASSETS_TO_TRACK.with(|a| a.borrow().clone());
    for asset in assets {
        let _ = fetch_price(asset).await;
    }
}

#[update]
fn set_registry(registry: Principal) {
    CONFIG.with(|config| {
        let mut config = config.borrow_mut();
        if caller() != config.owner {
            ic_cdk::trap("Only owner can set registry");
        }
        config.registry_canister = Some(registry);
    });
}

#[update]
fn set_clearing(clearing: Principal) {
    CONFIG.with(|config| {
        let mut config = config.borrow_mut();
        if caller() != config.owner {
            ic_cdk::trap("Only owner can set clearing");
        }
        config.clearing_canister = Some(clearing);
    });
}

#[update]
fn add_asset(asset_id: String) {
    CONFIG.with(|config| {
        if caller() != config.borrow().owner {
            ic_cdk::trap("Only owner can add assets");
        }
    });
    ASSETS_TO_TRACK.with(|a| {
        let mut assets = a.borrow_mut();
        if !assets.contains(&asset_id) {
            assets.push(asset_id);
        }
    });
}

#[update]
fn remove_asset(asset_id: String) {
    CONFIG.with(|config| {
        if caller() != config.borrow().owner {
            ic_cdk::trap("Only owner can remove assets");
        }
    });
    ASSETS_TO_TRACK.with(|a| {
        let mut assets = a.borrow_mut();
        assets.retain(|x| x != &asset_id);
    });
}

#[query]
fn get_assets() -> Vec<String> {
    ASSETS_TO_TRACK.with(|a| a.borrow().clone())
}

#[query]
fn get_latest_price(asset_id: String) -> Option<Price> {
    PRICES.with(|prices| prices.borrow().get(&asset_id).cloned())
}

#[update]
async fn fetch_price(asset_id: String) -> Result<Price, String> {
    // Only CoinGecko supported for now: "internet-computer" -> "icp"
    let coin_id = match asset_id.to_lowercase().as_str() {
        "icp" => "internet-computer",
        "btc" | "ckbtc" => "bitcoin",
        "eth" | "cketh" => "ethereum",
        _ => return Err(format!("Unsupported asset: {}", asset_id)),
    };

    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
        coin_id
    );

    let request_headers = vec![
        HttpHeader {
            name: "Host".to_string(),
            value: "api.coingecko.com".to_string(),
        },
        HttpHeader {
            name: "User-Agent".to_string(),
            value: "ic-pricer-canister".to_string(),
        },
    ];

    let request = CanisterHttpRequestArgument {
        url,
        max_response_bytes: Some(2000),
        method: HttpMethod::GET,
        headers: request_headers,
        body: None,
        transform: Some(TransformContext::from_name(
            "transform_response".to_string(),
            vec![],
        )),
    };

    // Cycles cost for HTTP outcalls depends on response size and subnet size.
    // For now, let's assume the caller provides enough or the canister has them.
    match http_request(request, 20_000_000_000).await {
        Ok((response,)) => {
            if response.status != 200u64 {
                return Err(format!(
                    "HTTP request failed with status {}",
                    response.status
                ));
            }

            let body_str = String::from_utf8(response.body)
                .map_err(|e| format!("Failed to decode body: {}", e))?;

            let json: Value = serde_json::from_str(&body_str)
                .map_err(|e| format!("Failed to parse JSON: {}", e))?;

            let price_val = json[coin_id]["usd"]
                .as_f64()
                .ok_or_else(|| "Price not found in response".to_string())?;

            // Convert to internal Price type (using 8 decimals as convention for now)
            let price = Price::new((price_val * 100_000_000.0) as u128, 8);

            PRICES.with(|prices| {
                prices.borrow_mut().insert(asset_id.clone(), price.clone());
            });

            // Push to clearing if configured
            let clearing = CONFIG.with(|c| c.borrow().clearing_canister);
            if let Some(clearing) = clearing {
                let asset_id_copy = asset_id.clone();
                let price_copy = price.clone();
                ic_cdk::spawn(async move {
                    let _: Result<(), (ic_cdk::api::call::RejectionCode, String)> = ic_cdk::call(
                        clearing,
                        "update_asset_price",
                        (UpdateAssetPriceParams {
                            asset_id: asset_id_copy,
                            price: price_copy,
                        },),
                    )
                    .await;
                });
            }

            Ok(price)
        }
        Err((code, msg)) => Err(format!("HTTP outcall error: {:?} - {}", code, msg)),
    }
}

#[query]
fn transform_response(args: TransformArgs) -> HttpResponse {
    let mut res = args.response;
    // Remove headers to ensure determinism across nodes
    res.headers = vec![];
    res
}

#[pre_upgrade]
fn pre_upgrade() {
    let prices = PRICES.with(|p| p.borrow().clone());
    let config = CONFIG.with(|c| c.borrow().clone());
    let assets = ASSETS_TO_TRACK.with(|a| a.borrow().clone());
    ic_cdk::storage::stable_save((prices, config, assets))
        .expect("Failed to save to stable storage");
}

#[post_upgrade]
fn post_upgrade() {
    let (prices, config, assets): (BTreeMap<String, Price>, PricerConfig, Vec<String>) =
        ic_cdk::storage::stable_restore().expect("Failed to restore from stable storage");
    PRICES.with(|p| *p.borrow_mut() = prices);
    CONFIG.with(|c| *c.borrow_mut() = config);
    ASSETS_TO_TRACK.with(|a| *a.borrow_mut() = assets);

    // Restart the timer after upgrade
    ic_cdk_timers::set_timer_interval(std::time::Duration::from_secs(300), || {
        ic_cdk::spawn(fetch_all_prices());
    });
}
