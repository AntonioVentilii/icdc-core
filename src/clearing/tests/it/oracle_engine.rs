use shared::types::{
    engine::EngineRole,
    oracle::{
        AddOracleParams, ManageOraclePrincipalsParams, Oracle, OracleError, OracleMetadata,
        OracleResult, UpdateOracleMetadataParams,
    },
};

use crate::utils::{
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait as _,
};

#[test]
fn oracle_admin_can_register_oracle() {
    let setup = TestSetup::default();
    let oracle_admin = test_user(80);

    let engine_id = setup.register_engine("Oracle Engine", vec![EngineRole::OracleAdmin]);
    setup.grant_engine_role(&engine_id, oracle_admin, EngineRole::OracleAdmin);
    setup.pic.tick();

    let res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "add_oracle",
            (AddOracleParams {
                oracle_id: "ENGINE_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Engine Oracle".to_owned(),
                    website: None,
                    description: None,
                },
                authorized_principals: vec![oracle_admin],
            },),
        )
        .unwrap();

    assert!(
        matches!(res, OracleResult::Ok),
        "OracleAdmin should be able to register an oracle"
    );
}

#[test]
fn creator_only_engine_cannot_register_oracle() {
    let setup = TestSetup::default();
    let creator = test_user(81);

    let engine_id = setup.register_engine("Creator Only Engine", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_id, creator, EngineRole::Creator);
    setup.pic.tick();

    let res: OracleResult = setup
        .registry
        .update(
            creator,
            "add_oracle",
            (AddOracleParams {
                oracle_id: "SHOULD_FAIL_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Should Fail".to_owned(),
                    website: None,
                    description: None,
                },
                authorized_principals: vec![creator],
            },),
        )
        .unwrap();

    assert!(
        matches!(
            res,
            OracleResult::Err(OracleError::UnauthorizedOracleManager)
        ),
        "Creator-only role should not be able to register oracles"
    );
}

#[test]
fn oracle_admin_can_update_metadata() {
    let setup = TestSetup::default();
    let oracle_admin = test_user(82);

    let engine_id = setup.register_engine("Oracle Meta Engine", vec![EngineRole::OracleAdmin]);
    setup.grant_engine_role(&engine_id, oracle_admin, EngineRole::OracleAdmin);
    setup.pic.tick();

    let add_res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "add_oracle",
            (AddOracleParams {
                oracle_id: "META_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Original Name".to_owned(),
                    website: None,
                    description: None,
                },
                authorized_principals: vec![],
            },),
        )
        .unwrap();
    assert!(matches!(add_res, OracleResult::Ok));
    setup.pic.tick();

    let update_res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "update_oracle_metadata",
            (UpdateOracleMetadataParams {
                oracle_id: "META_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Updated Name".to_owned(),
                    website: Some("https://example.com".to_owned()),
                    description: None,
                },
            },),
        )
        .unwrap();
    assert!(
        matches!(update_res, OracleResult::Ok),
        "OracleAdmin should be able to update oracle metadata"
    );

    let oracle: Option<Oracle> = setup
        .registry
        .query(oracle_admin, "get_oracle", ("META_ORACLE".to_owned(),))
        .unwrap();
    let oracle = oracle.expect("Oracle should exist");
    assert_eq!(oracle.metadata.name, "Updated Name");
}

#[test]
fn oracle_admin_can_manage_principals() {
    let setup = TestSetup::default();
    let oracle_admin = test_user(83);
    let new_principal = test_user(84);

    let engine_id =
        setup.register_engine("Oracle Principals Engine", vec![EngineRole::OracleAdmin]);
    setup.grant_engine_role(&engine_id, oracle_admin, EngineRole::OracleAdmin);
    setup.pic.tick();

    let add_res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "add_oracle",
            (AddOracleParams {
                oracle_id: "PRINCIPAL_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Test Oracle".to_owned(),
                    website: None,
                    description: None,
                },
                authorized_principals: vec![],
            },),
        )
        .unwrap();
    assert!(matches!(add_res, OracleResult::Ok));
    setup.pic.tick();

    let manage_res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "manage_oracle_principals",
            (ManageOraclePrincipalsParams {
                oracle_id: "PRINCIPAL_ORACLE".to_owned(),
                add_principals: vec![new_principal],
                remove_principals: vec![],
            },),
        )
        .unwrap();
    assert!(
        matches!(manage_res, OracleResult::Ok),
        "OracleAdmin should be able to manage oracle principals"
    );

    let is_authorized: bool = setup
        .registry
        .query(
            setup.controller,
            "is_oracle_authorized",
            ("PRINCIPAL_ORACLE".to_owned(), new_principal),
        )
        .unwrap();
    assert!(is_authorized, "Added principal should be authorized");
}

#[test]
fn random_user_cannot_register_oracle() {
    let setup = TestSetup::default();
    let random_user = test_user(85);

    let res: OracleResult = setup
        .registry
        .update(
            random_user,
            "add_oracle",
            (AddOracleParams {
                oracle_id: "RANDOM_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Random".to_owned(),
                    website: None,
                    description: None,
                },
                authorized_principals: vec![],
            },),
        )
        .unwrap();

    assert!(
        matches!(
            res,
            OracleResult::Err(OracleError::UnauthorizedOracleManager)
        ),
        "Random user should not be able to register oracles"
    );
}

// --- Error / edge-case tests ---

#[test]
fn duplicate_oracle_id_rejected() {
    let setup = TestSetup::default();
    let oracle_admin = test_user(86);

    let engine_id = setup.register_engine("Dup Oracle Engine", vec![EngineRole::OracleAdmin]);
    setup.grant_engine_role(&engine_id, oracle_admin, EngineRole::OracleAdmin);
    setup.pic.tick();

    let params = AddOracleParams {
        oracle_id: "DUP_ORACLE".to_owned(),
        metadata: OracleMetadata {
            name: "First".to_owned(),
            website: None,
            description: None,
        },
        authorized_principals: vec![],
    };

    let res1: OracleResult = setup
        .registry
        .update(oracle_admin, "add_oracle", (params.clone(),))
        .unwrap();
    assert!(matches!(res1, OracleResult::Ok));
    setup.pic.tick();

    let res2: OracleResult = setup
        .registry
        .update(oracle_admin, "add_oracle", (params,))
        .unwrap();
    assert!(
        matches!(res2, OracleResult::Err(OracleError::OracleAlreadyExists)),
        "Duplicate oracle ID should be rejected"
    );
}

#[test]
fn update_nonexistent_oracle_rejected() {
    let setup = TestSetup::default();
    let oracle_admin = test_user(87);

    let engine_id = setup.register_engine("Update Missing Engine", vec![EngineRole::OracleAdmin]);
    setup.grant_engine_role(&engine_id, oracle_admin, EngineRole::OracleAdmin);
    setup.pic.tick();

    let res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "update_oracle_metadata",
            (UpdateOracleMetadataParams {
                oracle_id: "NONEXISTENT_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Does Not Exist".to_owned(),
                    website: None,
                    description: None,
                },
            },),
        )
        .unwrap();

    assert!(
        matches!(res, OracleResult::Err(OracleError::OracleNotFound)),
        "Update on nonexistent oracle should return OracleNotFound"
    );
}

#[test]
fn manage_nonexistent_oracle_rejected() {
    let setup = TestSetup::default();
    let oracle_admin = test_user(88);

    let engine_id = setup.register_engine("Manage Missing Engine", vec![EngineRole::OracleAdmin]);
    setup.grant_engine_role(&engine_id, oracle_admin, EngineRole::OracleAdmin);
    setup.pic.tick();

    let res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "manage_oracle_principals",
            (ManageOraclePrincipalsParams {
                oracle_id: "NONEXISTENT_ORACLE".to_owned(),
                add_principals: vec![test_user(89)],
                remove_principals: vec![],
            },),
        )
        .unwrap();

    assert!(
        matches!(res, OracleResult::Err(OracleError::OracleNotFound)),
        "Manage on nonexistent oracle should return OracleNotFound"
    );
}

#[test]
fn remove_oracle_principal_removes_authorization() {
    let setup = TestSetup::default();
    let oracle_admin = test_user(90);
    let authorized = test_user(91);

    let engine_id = setup.register_engine("Remove Principal Engine", vec![EngineRole::OracleAdmin]);
    setup.grant_engine_role(&engine_id, oracle_admin, EngineRole::OracleAdmin);
    setup.pic.tick();

    let add_res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "add_oracle",
            (AddOracleParams {
                oracle_id: "REMOVE_PRINCIPAL_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Remove Test".to_owned(),
                    website: None,
                    description: None,
                },
                authorized_principals: vec![authorized],
            },),
        )
        .unwrap();
    assert!(matches!(add_res, OracleResult::Ok));
    setup.pic.tick();

    let is_authorized_before: bool = setup
        .registry
        .query(
            setup.controller,
            "is_oracle_authorized",
            ("REMOVE_PRINCIPAL_ORACLE".to_owned(), authorized),
        )
        .unwrap();
    assert!(
        is_authorized_before,
        "Principal should be authorized initially"
    );

    let manage_res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "manage_oracle_principals",
            (ManageOraclePrincipalsParams {
                oracle_id: "REMOVE_PRINCIPAL_ORACLE".to_owned(),
                add_principals: vec![],
                remove_principals: vec![authorized],
            },),
        )
        .unwrap();
    assert!(matches!(manage_res, OracleResult::Ok));
    setup.pic.tick();

    let is_authorized_after: bool = setup
        .registry
        .query(
            setup.controller,
            "is_oracle_authorized",
            ("REMOVE_PRINCIPAL_ORACLE".to_owned(), authorized),
        )
        .unwrap();
    assert!(
        !is_authorized_after,
        "Removed principal should no longer be authorized"
    );
}

#[test]
fn random_user_cannot_update_oracle_metadata() {
    let setup = TestSetup::default();
    let oracle_admin = test_user(92);
    let random_user = test_user(93);

    let engine_id = setup.register_engine("Random Update Engine", vec![EngineRole::OracleAdmin]);
    setup.grant_engine_role(&engine_id, oracle_admin, EngineRole::OracleAdmin);
    setup.pic.tick();

    let add_res: OracleResult = setup
        .registry
        .update(
            oracle_admin,
            "add_oracle",
            (AddOracleParams {
                oracle_id: "RANDOM_UPDATE_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Original".to_owned(),
                    website: None,
                    description: None,
                },
                authorized_principals: vec![],
            },),
        )
        .unwrap();
    assert!(matches!(add_res, OracleResult::Ok));
    setup.pic.tick();

    let res: OracleResult = setup
        .registry
        .update(
            random_user,
            "update_oracle_metadata",
            (UpdateOracleMetadataParams {
                oracle_id: "RANDOM_UPDATE_ORACLE".to_owned(),
                metadata: OracleMetadata {
                    name: "Hacked".to_owned(),
                    website: None,
                    description: None,
                },
            },),
        )
        .unwrap();

    assert!(
        matches!(
            res,
            OracleResult::Err(OracleError::UnauthorizedOracleManager)
        ),
        "Random user should not be able to update oracle metadata"
    );

    let oracle: Option<Oracle> = setup
        .registry
        .query(
            setup.controller,
            "get_oracle",
            ("RANDOM_UPDATE_ORACLE".to_owned(),),
        )
        .unwrap();
    assert_eq!(
        oracle.unwrap().metadata.name,
        "Original",
        "Name should remain unchanged after unauthorized update attempt"
    );
}
