use candid::{decode_one, encode_one, Principal};
use shared::types::{
    engine::{
        Engine, EngineError, EngineId, EngineResult, EngineRole, GrantEngineRoleParams,
        RegisterEngineParams, RegisterEngineResult, RevokeEngineRoleParams,
        UpdateEngineAdminsParams, UpdateEngineParams,
    },
    series::{AddSeriesParams, AddSeriesResult, SeriesError},
    BalanceDomain, Description, PayoffType, PayoutUnit, Series,
};

use crate::utils::{
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait as _,
};

fn add_series_as(
    setup: &TestSetup,
    caller: Principal,
    engine_id: Option<EngineId>,
) -> AddSeriesResult {
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
        engine_id,
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

    let res = add_series_as(&setup, creator_principal, Some(engine_id));
    assert!(
        matches!(res, AddSeriesResult::Ok(_)),
        "Engine creator should be able to create series"
    );
}

#[test]
fn non_engine_user_rejected() {
    let setup = TestSetup::default();
    let random_user = test_user(61);

    let res = add_series_as(&setup, random_user, None);
    assert!(
        matches!(res, AddSeriesResult::Err(SeriesError::EngineIdRequired)),
        "Non-controller without engine_id should be rejected"
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
                grantee: user,
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
                engine_id: engine_id.clone(),
                grantee: creator,
                role: EngineRole::Creator,
            },),
        )
        .unwrap();
    assert!(matches!(res, EngineResult::Ok));
    setup.pic.tick();

    let res = add_series_as(&setup, creator, Some(engine_id));
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

    let r1 = add_series_as(&setup, creator, Some(engine_id.clone()));
    assert!(matches!(r1, AddSeriesResult::Ok(_)));

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "revoke_engine_role",
            (RevokeEngineRoleParams {
                engine_id: engine_id.clone(),
                grantee: creator,
                role: EngineRole::Creator,
            },),
        )
        .unwrap();
    assert!(matches!(res, EngineResult::Ok));
    setup.pic.tick();

    let r2 = add_series_as(&setup, creator, Some(engine_id));
    assert!(
        matches!(r2, AddSeriesResult::Err(SeriesError::EngineRoleNotHeld)),
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

// --- Error / edge-case tests ---

#[test]
fn double_grant_returns_role_already_granted() {
    let setup = TestSetup::default();
    let user = test_user(66);

    let engine_id = setup.register_engine("Double Grant", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_id, user, EngineRole::Creator);
    setup.pic.tick();

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "grant_engine_role",
            (GrantEngineRoleParams {
                engine_id,
                grantee: user,
                role: EngineRole::Creator,
            },),
        )
        .unwrap();

    assert!(
        matches!(res, EngineResult::Err(EngineError::RoleAlreadyGranted)),
        "Double grant should return RoleAlreadyGranted"
    );
}

#[test]
fn revoke_without_grant_returns_role_not_granted() {
    let setup = TestSetup::default();
    let user = test_user(67);

    let engine_id = setup.register_engine("Revoke No Grant", vec![EngineRole::Creator]);

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "revoke_engine_role",
            (RevokeEngineRoleParams {
                engine_id,
                grantee: user,
                role: EngineRole::Creator,
            },),
        )
        .unwrap();

    assert!(
        matches!(res, EngineResult::Err(EngineError::RoleNotGranted)),
        "Revoke without prior grant should return RoleNotGranted"
    );
}

#[test]
fn register_engine_name_too_long() {
    let setup = TestSetup::default();

    let long_name = "x".repeat(129);

    let res: RegisterEngineResult = setup
        .registry
        .update(
            setup.controller,
            "register_engine",
            (RegisterEngineParams {
                name: long_name,
                description: None,
                icon_url: None,
                admins: vec![],
                allowed_roles: vec![EngineRole::Creator],
            },),
        )
        .unwrap();

    assert!(
        matches!(res, RegisterEngineResult::Err(EngineError::NameTooLong)),
        "Engine name over 128 chars should be rejected"
    );
}

#[test]
fn remove_engine_admins_cannot_remove_creator() {
    let setup = TestSetup::default();

    let engine_id = setup.register_engine("Creator Protect", vec![EngineRole::Creator]);

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "remove_engine_admins",
            (UpdateEngineAdminsParams {
                engine_id,
                principals: vec![setup.controller],
            },),
        )
        .unwrap();

    assert!(
        matches!(res, EngineResult::Err(EngineError::CannotRemoveCreator)),
        "Removing the Engine creator as admin should be rejected"
    );
}

#[test]
fn update_engine_metadata() {
    let setup = TestSetup::default();

    let engine_id = setup.register_engine("Original Name", vec![EngineRole::Creator]);

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "update_engine",
            (UpdateEngineParams {
                engine_id: engine_id.clone(),
                name: Some("Updated Name".to_owned()),
                description: Some(Some("A description".to_owned())),
                icon_url: None,
            },),
        )
        .unwrap();
    assert!(matches!(res, EngineResult::Ok));
    setup.pic.tick();

    let engine: Option<Engine> = setup
        .registry
        .query(setup.controller, "get_engine", (engine_id,))
        .unwrap();

    let engine = engine.expect("Engine should exist");
    assert_eq!(engine.name, "Updated Name");
    assert_eq!(engine.description, Some("A description".to_owned()));
}

#[test]
fn update_engine_name_too_long() {
    let setup = TestSetup::default();

    let engine_id = setup.register_engine("Short Name", vec![EngineRole::Creator]);

    let res: EngineResult = setup
        .registry
        .update(
            setup.controller,
            "update_engine",
            (UpdateEngineParams {
                engine_id,
                name: Some("x".repeat(129)),
                description: None,
                icon_url: None,
            },),
        )
        .unwrap();

    assert!(
        matches!(res, EngineResult::Err(EngineError::NameTooLong)),
        "update_engine with name > 128 chars should be rejected"
    );
}

#[test]
fn non_admin_cannot_grant_role() {
    let setup = TestSetup::default();
    let random_user = test_user(68);
    let target = test_user(69);

    let engine_id = setup.register_engine("No Access", vec![EngineRole::Creator]);

    let res: EngineResult = setup
        .registry
        .update(
            random_user,
            "grant_engine_role",
            (GrantEngineRoleParams {
                engine_id,
                grantee: target,
                role: EngineRole::Creator,
            },),
        )
        .unwrap();

    assert!(
        matches!(res, EngineResult::Err(EngineError::Unauthorized)),
        "Non-admin should not be able to grant roles"
    );
}

#[test]
fn engine_creator_with_correct_engine_id_creates_series() {
    let setup = TestSetup::default();
    let creator = test_user(90);

    let engine_id = setup.register_engine("EID Engine", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_id, creator, EngineRole::Creator);
    setup.pic.tick();

    let res = add_series_as(&setup, creator, Some(engine_id.clone()));
    let AddSeriesResult::Ok(series_id) = res else {
        panic!("Creator with correct engine_id should succeed");
    };

    let series: Option<Series> = setup
        .registry
        .query(setup.controller, "get_series", (series_id,))
        .unwrap();

    let series = series.expect("Series should exist");
    assert_eq!(
        series.engine_id,
        Some(engine_id),
        "Series should carry the engine_id it was created with"
    );
}

#[test]
fn engine_creator_with_wrong_engine_id_rejected() {
    let setup = TestSetup::default();
    let creator = test_user(91);

    let engine_a = setup.register_engine("Engine A", vec![EngineRole::Creator]);
    let engine_b = setup.register_engine("Engine B", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_a, creator, EngineRole::Creator);
    setup.pic.tick();

    let res = add_series_as(&setup, creator, Some(engine_b));
    assert!(
        matches!(res, AddSeriesResult::Err(SeriesError::EngineRoleNotHeld)),
        "Creator on engine A should be rejected for engine B"
    );
}

#[test]
fn non_controller_without_engine_id_rejected_for_non_social() {
    let setup = TestSetup::default();
    let creator = test_user(92);

    let engine_id = setup.register_engine("No EID Engine", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_id, creator, EngineRole::Creator);
    setup.pic.tick();

    let res = add_series_as(&setup, creator, None);
    assert!(
        matches!(res, AddSeriesResult::Err(SeriesError::EngineIdRequired)),
        "Non-controller without engine_id should be rejected for non-social market"
    );
}
