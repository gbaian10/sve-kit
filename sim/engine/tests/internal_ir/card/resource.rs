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
            ResourceOperation::Add,
            1,
        ),
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::Ep,
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
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([{"pp":pp}]);
        ExpandedLowerer::new().unwrap().lower(&tree).unwrap_err();
    }
}
