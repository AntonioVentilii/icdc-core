use std::fs;

use candid::{Decode, Encode, Principal};
use pocket_ic::PocketIc;

#[test]
fn test_pricer_management() {
    let pic = PocketIc::new();
    let pricer_wasm =
        fs::read("../../target/wasm32-unknown-unknown/debug/pricer.wasm").expect("WASM not found");

    let pricer_id = pic.create_canister();
    pic.add_cycles(pricer_id, 10_000_000_000_000);

    let registry_id = Principal::from_slice(&[1; 29]);
    let init_args = Encode!(&Some(registry_id)).unwrap();

    // Install canister. The default caller is anonymous or a specific PocketIC principal.
    pic.install_canister(pricer_id, pricer_wasm, init_args, None);

    // The owner in pricer/lib.rs will be whatever the installation caller was.
    // In pocket-ic, we can act as anonymous or any principal.
    // Let's assume the installation caller was anonymous (default).
    let owner = Principal::anonymous();

    // Check initial assets
    let res = pic
        .query_call(pricer_id, owner, "get_assets", Encode!().unwrap())
        .expect("get_assets failed");
    let assets: Vec<String> = Decode!(&res, Vec<String>).unwrap();
    assert!(assets.contains(&"icp".to_string()));
    assert!(assets.contains(&"ckbtc".to_string()));
    assert!(assets.contains(&"cketh".to_string()));

    // Test add_asset (as owner)
    pic.update_call(
        pricer_id,
        owner,
        "add_asset",
        Encode!(&"sol".to_string()).unwrap(),
    )
    .expect("add_asset failed");

    // Verify added asset
    let res = pic
        .query_call(pricer_id, owner, "get_assets", Encode!().unwrap())
        .expect("get_assets (after add) failed");
    let assets: Vec<String> = Decode!(&res, Vec<String>).unwrap();
    assert!(assets.contains(&"sol".to_string()));

    // Test remove_asset
    pic.update_call(
        pricer_id,
        owner,
        "remove_asset",
        Encode!(&"icp".to_string()).unwrap(),
    )
    .expect("remove_asset failed");

    let res = pic
        .query_call(pricer_id, owner, "get_assets", Encode!().unwrap())
        .expect("get_assets (after remove) failed");
    let assets: Vec<String> = Decode!(&res, Vec<String>).unwrap();
    assert!(!assets.contains(&"icp".to_string()));
}
