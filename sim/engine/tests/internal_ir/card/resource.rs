use super::*;
use sve_engine::game::internal::ResourceOutcome;
use sve_engine::ir::lowering::ExpandedLowerer;

fn recovery(
    id: u64,
    player: PlayerRef,
    resource: Resource,
    operation: ResourceOperation,
    amount: i64,
) -> Located<Node> {
    instruction(
        id,
        Action::Resource {
            player,
            resource,
            operation,
            amount: Expr::Literal(Int(amount)),
        },
    )
}

#[test]
fn pp_recovery_retains_actual_capped_results_without_payment_completion() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(seat, json!([]));
        initial["players"][actor]["pp"] = json!({"current":3,"max":5});
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "pp-recovery",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![
                    recovery(
                        1,
                        PlayerRef::Controller {},
                        Resource::PpCurrent,
                        ResourceOperation::Add,
                        3,
                    ),
                    recovery(
                        2,
                        PlayerRef::Controller {},
                        Resource::PpCurrent,
                        ResourceOperation::Add,
                        i64::MAX,
                    ),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        for (id, requested, before, actual) in [(1, 3, 3, 2), (2, i64::MAX, 5, 0)] {
            assert_eq!(
                result.receipts[&ReceiptId(id)],
                InstructionOutcome::Resource(ResourceOutcome {
                    player: PlayerId { seat },
                    resource: Resource::PpCurrent,
                    requested: Int(requested),
                    before: Int(before),
                    after: Int(5),
                    actual: Int(actual),
                })
            );
        }
        assert_eq!(result.facts, Vec::<Fact>::new());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[actor]["pp"], json!({"current":5,"max":5}));
        assert_eq!(state[opponent]["pp"], json!({"current":2,"max":2}));
    }
}

#[test]
fn body_resource_admission_rejects_payment_other_resources_and_other_players_atomically() {
    assert!(
        WellFormedProgram::check(body(
            1,
            vec![recovery(
                1,
                PlayerRef::Controller {},
                Resource::PpCurrent,
                ResourceOperation::Pay,
                1,
            )]
        ))
        .is_err(),
        "payment requires a cost context"
    );
    for step in [
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::PpMaximum,
            ResourceOperation::Set,
            1,
        ),
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::Sep,
            ResourceOperation::Add,
            1,
        ),
        recovery(
            1,
            PlayerRef::Active {},
            Resource::PpCurrent,
            ResourceOperation::Add,
            1,
        ),
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::PpCurrent,
            ResourceOperation::Set,
            1,
        ),
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::PpCurrent,
            ResourceOperation::Add,
            -1,
        ),
    ] {
        let mut game = game(1, 0);
        let before = format!("{game:?}");
        game.execute_internal(&checked(vec![step]), &source("synthetic"), context(1))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "rejected body resources must not mutate state"
        );
    }
}

#[test]
fn pp_author_lowering_rejects_dynamic_negative_zero_and_to_max() {
    for pp in [
        json!({"who":"opp","delta":3}),
        json!({"who":"self","delta":-1}),
        json!({"who":"self","delta":0}),
        json!({"who":"self","delta":{"resource":{"who":"self","kind":"pp"}}}),
        json!({"who":"self","to_max":true}),
    ] {
        for kind in ["pp", "pp_max", "ep"] {
            let mut tree = fanfare::expanded::tree().clone();
            tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([{kind:pp}]);
            ExpandedLowerer::new().unwrap().lower(&tree).unwrap_err();
        }
    }
}

#[test]
fn maximum_pp_increases_only_the_cap_and_records_actual_zero_at_ten() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        for maximum in [3_i64, 9, 10] {
            for current in [0, maximum] {
                for requested in [2_i64, i64::MAX] {
                    let mut initial = setup(seat, json!([]));
                    initial["players"][actor]["pp"] = json!({"current":current,"max":maximum});
                    let mut game = Game::new(
                        catalog(),
                        &initial,
                        &Value::Null,
                        &Value::Null,
                        "maximum-pp",
                    )
                    .unwrap();
                    let result = game
                        .execute_internal(
                            &checked(vec![recovery(
                                1,
                                PlayerRef::Controller {},
                                Resource::PpMaximum,
                                ResourceOperation::Add,
                                requested,
                            )]),
                            &source("synthetic"),
                            context(seat),
                        )
                        .unwrap();
                    let actual = requested.min(10_i64.saturating_sub(maximum));
                    let after = maximum.saturating_add(actual);
                    assert_eq!(result.receipts.len(), 1);
                    assert_eq!(
                        result.receipts[&ReceiptId(1)],
                        InstructionOutcome::Resource(ResourceOutcome {
                            player: PlayerId { seat },
                            resource: Resource::PpMaximum,
                            requested: Int(requested),
                            before: Int(maximum),
                            after: Int(after),
                            actual: Int(actual),
                        })
                    );
                    assert!(
                        result.facts.is_empty(),
                        "resource increases do not create payment facts"
                    );
                    let state = game.projection(View::Referee).unwrap();
                    assert_eq!(state[actor]["pp"], json!({"current":current,"max":after}));
                    assert_eq!(state[opponent]["pp"], json!({"current":2,"max":2}));
                }
            }
        }
    }
}

#[test]
fn ep_body_increases_are_uncapped_and_do_not_change_pp_sep_or_the_other_player() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(seat, json!([]));
        initial["players"][actor]["ep"] = json!(2_i64);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "ep-increase",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![
                    recovery(
                        1,
                        PlayerRef::Controller {},
                        Resource::Ep,
                        ResourceOperation::Add,
                        11,
                    ),
                    recovery(
                        2,
                        PlayerRef::Controller {},
                        Resource::Ep,
                        ResourceOperation::Add,
                        1,
                    ),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        for (id, requested, before, after) in [(1, 11, 2, 13), (2, 1, 13, 14)] {
            assert_eq!(
                result.receipts[&ReceiptId(id)],
                InstructionOutcome::Resource(ResourceOutcome {
                    player: PlayerId { seat },
                    resource: Resource::Ep,
                    requested: Int(requested),
                    before: Int(before),
                    after: Int(after),
                    actual: Int(requested),
                })
            );
        }
        assert_eq!(result.facts, Vec::<Fact>::new());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[actor]["ep"], 14_i64);
        assert_eq!(state[opponent]["ep"], 0_i64);
        assert_eq!(state[actor]["sep"], 0_i64);
        assert_eq!(state[actor]["pp"], json!({"current":2,"max":2}));
    }
}

#[test]
fn ep_body_overflow_and_non_controller_requests_preserve_authority() {
    for seat in [1, 2] {
        for (player, amount, expected) in [
            (PlayerRef::Controller {}, i64::MAX, "integer representation"),
            (
                PlayerRef::Active {},
                1,
                "fixed positive own resource increase",
            ),
        ] {
            let actor = if seat == 1 { "P1" } else { "P2" };
            let mut initial = setup(seat, json!([]));
            initial["players"][actor]["ep"] = json!(2_i64);
            let mut game =
                Game::new(catalog(), &initial, &Value::Null, &Value::Null, "ep-reject").unwrap();
            let before = format!("{game:?}");
            let failure = game
                .execute_internal(
                    &checked(vec![recovery(
                        1,
                        player,
                        Resource::Ep,
                        ResourceOperation::Add,
                        amount,
                    )]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap_err();
            assert!(
                failure.to_string().contains(expected),
                "the intended EP boundary rejects"
            );
            assert!(
                format!("{game:?}") == before,
                "rejected EP changes preserve authority"
            );
        }
    }
}
