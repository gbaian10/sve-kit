#![expect(
    clippy::panic,
    reason = "Unexpected fixture shapes must fail the test."
)]
use super::*;

fn guarded() -> Located<Ability> {
    let mut value = payment::paid_ability(vec![payment::pp(1), payment::act()], vec![draw(1, 1)]);
    let play = value.node.play.as_mut().unwrap();
    play.limit = Some(LimitSpec {
        maximum: Nat(1),
        window: LimitWindow::Turn,
        scope: LimitScope::AbilityInstance,
        consume_at: ConsumptionPoint::PlayCommitted,
    });
    let CostSpec::Ordered { steps } = &mut play.costs else {
        panic!("ordered costs")
    };
    steps[0].instruction.precondition = Some(Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        protects: steps[0].instruction.id,
        reason: PreconditionReason::CardText {},
        predicate: Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Live(Box::new(IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::Life,
            })),
            comparison: Comparison::LessEqual,
            right: Expr::Literal(Int(19)),
        })),
    });
    value
}

#[test]
fn refused_use_spends_neither_ordered_costs_nor_once_per_turn_quota() {
    for seat in [1, 2] {
        let lower_life = ability(
            2,
            vec![instruction(
                2,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(-1)),
                    },
                },
            )],
        );
        let mut game = position_with_card(seat, &card(vec![guarded(), lower_life]));
        let before = format!("{game:?}");
        let error = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert!(
            matches!(error, sve_engine::EngineFailure::IllegalPlay(_)),
            "the prerequisite refuses use"
        );
        assert!(
            format!("{game:?}") == before,
            "no PP, posture, clock or quota changes on refusal"
        );
        resolved(
            game.activate_internal_card("s", AbilityId(2), PlayerId { seat })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        activate(&mut game, seat);
        let actor = if seat == 1 { "P1" } else { "P2" };
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state[actor]["pp"]["current"], 1_u8,
            "the later legal attempt pays PP once"
        );
        assert_eq!(
            state["objects"]["s"]["acted"], true,
            "the later attempt can still use its quota and act"
        );
        assert_eq!(
            state[actor]["hand"],
            json!(["d"]),
            "the legal body resolves"
        );
    }
}

#[test]
fn only_the_first_cost_may_carry_a_card_text_use_prerequisite() {
    for change in 0..3 {
        let mut value = guarded();
        let CostSpec::Ordered { steps } = &mut value.node.play.as_mut().unwrap().costs else {
            panic!("costs")
        };
        let second = steps[1].instruction.id;
        let guard = steps[0].instruction.precondition.as_mut().unwrap();
        if change == 0 {
            guard.protects = second;
        }
        if change == 1 {
            guard.kind = PreconditionKind::ForcedZeroUnits;
        }
        if change == 2 {
            let mut guard = steps[0].instruction.precondition.take().unwrap();
            guard.protects = second;
            steps[1].instruction.precondition = Some(guard);
        }
        assert!(
            WellFormedCardProgram::check(CardProgram {
                abilities: vec![value],
                card_play_costs: vec![]
            })
            .is_err(),
            "a per-step guard cannot silently turn payment into a partial transaction"
        );
    }
}

#[test]
fn author_use_prerequisites_reject_hidden_filters_and_later_resolution_values() {
    for condition in [
        json!({"cmp":{"left":{"count":{"who":"self","zone":"hand","kind":"spell"}},"op":"ge","right":1}}),
        json!({"cmp":{"left":{"at":"resolution","value":{"count":{"who":"self","zone":"cemetery"}}},"op":"ge","right":1}}),
    ] {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["BP01-041"]["abilities"][0]["activate"]["usable_if"] = condition;
        assert!(
            lowerer().lower(&tree).is_err(),
            "a use gate must be evaluable at use without private inspection"
        );
    }
}
