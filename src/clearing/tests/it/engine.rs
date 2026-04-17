use candid::{decode_one, encode_one, Principal};
use shared::types::{
    engine::{
        Engine, EngineError, EngineResult, EngineRole, GrantEngineRoleParams,
        RevokeEngineRoleParams, UpdateEngineAdminsParams,
    },
    series::{AddSeriesParams, AddSeriesResult, SeriesError},
    BalanceDomain, Description, PayoffType, PayoutUnit,
};

use crate::utils::{
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait as _,
};

fn add_series_as(setup: &TestSetup, caller: Principal) -> AddSeriesResult {
    let params = AddSeriesParams {
        underlying: "ETH".to_owned(),
        balance_domain: BalanceDomain::Settlement,
        expiry_ns: 3_000_000_000_000_000_000,
        payoff_type: PayoffType::Binary,
        strike: None,
        price_precision: 6,
        payout_unit: PayoutUnit::usd(),
        oracle_source: "Chainlink".to_owned(),
        title: "ETH Binary".to_owned(),
        description: Description::plain("Test"),
        outcomes: None,
        icon_url: None,
        banner_url: None,
        trading_access: vec![],
    };

    let res_bytes = setup
        .pic
        .update_call(
            setup.registry.canister_id(),
            caller,
            "add_series",
            encode_one(params).unwrap(),
        )
        .expect("add_series call failed");

    decode_one(&res_bytes).unwrap()
}

#[test]
fn engine_lifecycle_register_grant_create() {
    let setup = TestSetup::default();
    let creator_principal = test_user(60);

    let engine_id = setup.register_engine(
        "Test Engine",
        vec![EngineRole::Creator, EngineRole::OracleAdmin],
    );

    setup.grant_engine_role(&engine_id, creator_principal, EngineRole::Creator);
    setup.pic.tick();

    let res = add_series_as(&setup, creator_principal);
    assert!(
        matches!(res, AddSeriesResult::Ok(_)),
        "Engine creator should be able to create series"
    );
}

#[test]
fn non_engine_user_rejected() {
    let setup = TestSetup::default();
    let random_user = test_user(61);

    let res = add_series_as(&setup, random_user);
    assert!(
        matches!(res, AddSeriesResult::Err(SeriesError::Unauthorized)),
        "Non-engine user should be rejected"
    );
}

#[test]
fn allowed_roles_scoping_rejects_wrong_role() {
    let setup = TestSetup::default();
    let user = test_user(62);

    let engine_id = setup.register_engine("Creator-Only Engine", vec![EngineRole::Creator]);

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "grant_engine_role",
            (GrantEngineRoleParams {
                engine_id,
                principal: user,
                role: EngineRole::OracleAdmin,
            },),
        )
        .unwrap();

    assert!(
        matches!(res, EngineResult::Err(EngineError::RoleNotAllowed)),
        "Should reject OracleAdmin grant on Creator-only engine"
    );
}

#[test]
fn engine_admin_can_grant_roles() {
    let setup = TestSetup::default();
    let admin = test_user(63);
    let creator = test_user(64);

    let engine_id = setup.register_engine("Admin Test", vec![EngineRole::Creator]);

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "add_engine_admins",
            (UpdateEngineAdminsParams {
                engine_id: engine_id.clone(),
                principals: vec![admin],
            },),
        )
        .unwrap();
    assert!(matches!(res, EngineResult::Ok));
    setup.pic.tick();

    let res: EngineResult = setup
        .registry
        .update(
            admin,
            "grant_engine_role",
            (GrantEngineRoleParams {
                engine_id,
                principal: creator,
                role: EngineRole::Creator,
            },),
        )
        .unwrap();
    assert!(matches!(res, EngineResult::Ok));
    setup.pic.tick();

    let res = add_series_as(&setup, creator);
    assert!(
        matches!(res, AddSeriesResult::Ok(_)),
        "Creator granted by admin should work"
    );
}

#[test]
fn revoke_role_removes_access() {
    let setup = TestSetup::default();
    let creator = test_user(65);

    let engine_id = setup.register_engine("Revoke Test", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_id, creator, EngineRole::Creator);
    setup.pic.tick();

    let r1 = add_series_as(&setup, creator);
    assert!(matches!(r1, AddSeriesResult::Ok(_)));

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "revoke_engine_role",
            (RevokeEngineRoleParams {
                engine_id,
                principal: creator,
                role: EngineRole::Creator,
            },),
        )
        .unwrap();
    assert!(matches!(res, EngineResult::Ok));
    setup.pic.tick();

    let r2 = add_series_as(&setup, creator);
    assert!(
        matches!(r2, AddSeriesResult::Err(SeriesError::Unauthorized)),
        "Revoked user should be rejected"
    );
}

#[test]
fn list_engines() {
    let setup = TestSetup::default();

    setup.register_engine("Engine A", vec![EngineRole::Creator]);
    setup.register_engine("Engine B", vec![EngineRole::OracleAdmin]);
    setup.pic.tick();

    let engines: Vec<Engine> = setup
        .registry
        .query(setup.controller, "list_engines", ())
        .unwrap();

    assert_eq!(engines.len(), 2);
    assert_eq!(engines[0].name, "Engine A");
    assert_eq!(engines[1].name, "Engine B");
}
