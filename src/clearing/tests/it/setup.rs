use candid::Principal;
use ic_cdk::println;
use shared::{constants::VUSD_LEDGER, types::minter::ConfigResult};

use crate::utils::{pic_canister::PicCanisterTrait as _, test_environment::TestSetup};

#[test]
fn multi_canister_setup() {
    let env = TestSetup::default();

    println!("Clearing canister: {}", env.clearing.canister_id());
    println!("Registry canister: {}", env.registry.canister_id());
    println!("Minter canister: {}", env.minter.canister_id());
}

#[test]
fn clearing_is_linked_to_registry() {
    let env = TestSetup::default();

    let registry_principal: Principal = env
        .clearing
        .query(env.controller, "get_registry_canister", ())
        .expect("get_registry_canister failed");

    assert_eq!(registry_principal, env.registry.canister_id());
}

#[test]
fn clearing_has_ledger_canister() {
    let env = TestSetup::default();

    let result: ConfigResult = env
        .minter
        .query(env.controller, "config", ())
        .expect("failed to get minter config");

    let config = match result {
        ConfigResult::Ok(c) => c,
        ConfigResult::Err(e) => panic!("minter config error: {e}"),
    };

    assert_eq!(
        config.ledger_canister,
        Principal::from_text(VUSD_LEDGER).unwrap()
    );
}

#[test]
fn minter_has_authorized_callers() {
    let env = TestSetup::default();

    let result: ConfigResult = env
        .minter
        .query(env.controller, "config", ())
        .expect("failed to get minter config");

    let config = match result {
        ConfigResult::Ok(c) => c,
        ConfigResult::Err(e) => panic!("minter config error: {e}"),
    };

    assert!(config.authorized_callers.contains(&env.controller));
    assert!(config
        .authorized_callers
        .contains(&env.clearing.canister_id()));
}
