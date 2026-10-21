//! Search and Look reduction fragments; targets and full printed abilities remain unadmitted.

use super::*;
use sve_engine::game::internal::SearchAnswer;

fn catalogue() -> Arc<Catalog> {
    let snapshot = [("unit-spell", "スペル", "1"), ("unit-follower", "フォロワー", "3")].iter().map(|(number, kind, cost)| {
        json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string()
    }).collect::<Vec<_>>().join("\n");
    let effects = json!({"format":1,"kind":"effect_set","cards":{
        "unit-spell":{"status":"complete","review":"synthetic","abilities":[]},
        "unit-follower":{"status":"complete","review":"synthetic","abilities":[]}
    }})
    .to_string();
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string(),
            &[("cost".into(), effects)],
        )
        .unwrap(),
    )
}

fn position(seat: u32, full: bool) -> Game {
    let actor = seat_name(seat);
    let mut initial = setup(
        seat,
        json!([{"id":"d","card":"unit-follower"}, {"id":"next-draw","card":"unit-spell"}]),
    );
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["pp"] = json!({"current":3,"max":3});
    initial["players"][if seat == 1 { "P2" } else { "P1" }]["zones"]["deck"] =
        json!([{"id":"other-draw","card":"unit-spell"}]);
    if full {
        initial["players"][actor]["zones"]["ex"] = json!(
            (0..5)
                .map(|i| json!({"id":format!("e{i}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
    }
    Game::new(
        catalogue(),
        &initial,
        &Value::Null,
        &Value::Null,
        "cost-fragment",
    )
    .unwrap()
}

const fn seat_name(seat: u32) -> &'static str {
    if seat == 1 { "P1" } else { "P2" }
}

pub(super) fn reduce(id: u64, moved: MovementReceipt) -> Located<Node> {
    let affected = Binding::new(90, ScopeId(90));
    let duration = Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    instruction(
        id,
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects {
                objects: Selector::MoveAfter {
                    receipt: moved,
                    permission: FollowPermission::SameEffectExplicitReference {},
                },
                affected,
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::CostModifier {
                subjects: Selector::Bound {
                    reference: affected,
                },
                change: CostChange::Delta {
                    amount: Expr::Literal(Int(-2)),
                },
                duration: duration.clone(),
                usage: None,
            }),
            duration,
        },
    )
}

fn end(game: &mut Game) {
    game.submit(&json!({"do":"end-phase"})).unwrap();
    game.submit(&json!({"do":"pass"})).unwrap();
}

#[test]
fn search_successful_after_reduces_actual_play_cost_until_current_turn_end() {
    for seat in [1, 2] {
        let mut game = position(seat, false);
        let program = checked(vec![
            search::search(
                search::conditional(),
                search::cardinality(1, 1),
                MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex,
                    },
                },
            ),
            reduce(
                2,
                MovementReceipt::Search {
                    receipt: ReceiptReference::new(ReceiptId(1)),
                },
            ),
        ]);
        game.execute_internal(&program, &source("CP04-060"), context(seat))
            .unwrap();
        let question = game.internal_decision(View::Referee).unwrap();
        let result = game
            .answer_internal(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0])),
            )
            .unwrap();
        assert!(
            matches!(&result.receipts[&ReceiptId(2)], InstructionOutcome::Installed(subjects) if subjects.len() == 1)
        );
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::StatChange(_)))
                .count(),
            0
        );
        let mut expired = game.clone();
        game.submit(&json!({"do":"play","card":"d"})).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat_name(seat)]["pp"]["current"],
            2
        );
        end(&mut expired);
        end(&mut expired);
        let before = expired.projection(View::Referee).unwrap()[seat_name(seat)]["pp"]["current"]
            .as_i64()
            .unwrap();
        expired.submit(&json!({"do":"play","card":"d"})).unwrap();
        assert_eq!(
            expired.projection(View::Referee).unwrap()[seat_name(seat)]["pp"]["current"],
            before - 3
        );
    }
}

#[test]
fn look_successful_after_installs_but_a_full_ex_keeps_zero_subjects() {
    for (seat, full) in [(1, false), (2, true)] {
        let mut game = position(seat, full);
        let move_step = instruction(
            2,
            Action::Move {
                objects: ordering::looked(1),
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Ex,
                        },
                    },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Put,
            },
        );
        let result = game
            .execute_internal(
                &checked(vec![
                    ordering::look(1, 1),
                    move_step,
                    reduce(
                        3,
                        MovementReceipt::Move {
                            receipt: ReceiptReference::new(ReceiptId(2)),
                        },
                    ),
                ]),
                &source("BP18-059"),
                context(seat),
            )
            .unwrap();
        let Some(InstructionOutcome::Installed(subjects)) = result.receipts.get(&ReceiptId(3))
        else {
            panic!("cost installation");
        };
        assert_eq!(subjects.len(), usize::from(!full));
        if !full {
            game.submit(&json!({"do":"play","card":"d"})).unwrap();
            assert_eq!(
                game.projection(View::Referee).unwrap()[seat_name(seat)]["pp"]["current"],
                2
            );
        }
    }
}
