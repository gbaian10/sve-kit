//! Fixed Fusion quantities use the existing material payment and carry path.
#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these Fusion scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn program(count: u64) -> WellFormedCardProgram {
    lowerer().lower(&evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "fusion":{"sel":{"who":"self","zone":["hand","ex"],"trait":"財宝","orig_cost_ge":1},"n":count,"result":"paid"},
            "do":[
                {"draw":{"n":{"count":{"in":{"objects_of":"paid","view":"before"},"kind":"follower"}}}},
                {"counter":{"on":"this","name":"completed","delta":1}}
            ]
        }),
    )])).unwrap().remove("unit-follower").unwrap()
}

fn catalogue() -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" || row["number"] == "unit-spell" {
                row["faces"][0]["traits"] = json!(["財宝"]);
            }
            if row["number"] == "unit-spell" {
                row["faces"][0]["cost"] = json!("0");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    )
}

fn position(
    program: &WellFormedCardProgram,
    seat: u32,
    hand: usize,
    ex: usize,
    full_ex: bool,
) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["field"] = json!([]);
    let mut cards = vec![json!({"id":"source","card":"unit-follower"})];
    cards.extend((0..hand).map(|n| json!({"id":format!("h{n}"),"card":"unit-follower"})));
    initial["players"][actor]["zones"]["hand"] = json!(cards);
    let mut cards = (0..ex)
        .map(|n| json!({"id":format!("e{n}"),"card":"unit-follower"}))
        .collect::<Vec<_>>();
    if full_ex {
        cards.extend((ex..5).map(|n|json!({"id":format!("filler{n}"),"card":if n==4 {"unit-amulet"} else {"unit-spell"}})));
    }
    initial["players"][actor]["zones"]["ex"] = json!(cards);
    initial["players"][actor]["zones"]["deck"] = json!(
        (0..5)
            .map(|n| json!({"id":format!("d{n}"),"card":"unit-spell"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalogue(),
        &initial,
        &Value::Null,
        &Value::Null,
        "fixed-fusion",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn question(game: &mut Game, program: &WellFormedCardProgram, seat: u32) -> DecisionView {
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat },
        )
        .unwrap()
    else {
        panic!("fixed Fusion material question")
    };
    question
}

#[test]
fn fixed_positive_fusion_quantities_pay_exactly_and_follow_source_both_seats() {
    for count in [2_u64, 3, 4] {
        let program = program(count);
        for seat in [1, 2] {
            let actor = if seat == 1 { "P1" } else { "P2" };
            let mut game = position(&program, seat, 1, usize::try_from(count - 1).unwrap(), true);
            let question = question(&mut game, &program, seat);
            let InputDomain::CostMaterials(domain) = &question.domain else {
                panic!("materials")
            };
            assert_eq!(
                domain.cardinality,
                Cardinality {
                    minimum: Nat(count),
                    maximum: Some(Nat(count))
                }
            );
            assert_eq!(
                domain.members.len(),
                usize::try_from(count).unwrap(),
                "the matching source, zero-cost Treasure and other-trait fillers are excluded"
            );
            assert!(
                game.internal_decision(if seat == 1 { View::P2 } else { View::P1 })
                    .is_none()
            );
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(domain.members.iter().map(|m| m.handle).collect()),
                )
                .unwrap(),
            );
            assert!(result.progress.receipts.values().any(|outcome| matches!(outcome,InstructionOutcome::Payment {quantity,subjects,..} if quantity.unit==QuantityUnit::Cards && quantity.amount==Int(i64::try_from(count).unwrap()) && subjects.len()==usize::try_from(count).unwrap())));
            assert!(result.progress.facts.iter().any(|fact|matches!(fact,Fact::Fused(event) if event.materials.len()==usize::try_from(count).unwrap())));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[actor]["hand"].as_array().unwrap().len(),
                usize::try_from(count).unwrap()
            );
            assert_eq!(state["objects"]["source"]["counters"]["completed"], 1);
            assert_eq!(
                state[actor]["cemetery"].as_array().unwrap().len(),
                usize::try_from(count).unwrap()
            );
            assert!(game.internal_decision(View::Referee).is_none());
        }
    }
}

#[test]
fn exact_three_with_four_candidates_rejects_bad_answers_without_changing_pending_authority() {
    let program = program(3);
    let mut game = position(&program, 1, 1, 3, false);
    let question = question(&mut game, &program, 1);
    let before = exact_authority(&game);
    for handles in [vec![0, 1], vec![0, 1, 2, 3], vec![0, 0, 1], vec![0, 1, 99]] {
        assert!(matches!(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(handles)
            ),
            Err(sve_engine::EngineFailure::Invalid(_))
        ));
        assert_eq!(exact_authority(&game), before);
    }
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 1, 2]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert!(state["P1"]["ex"].as_array().unwrap().contains(&json!("e2")));
    assert_eq!(state["objects"]["source"]["counters"]["completed"], 1);
}

#[test]
fn an_unaffordable_exact_three_restores_all_authority_before_asking() {
    let program = program(3);
    let mut game = position(&program, 1, 1, 1, false);
    let before = exact_authority(&game);
    assert!(matches!(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 }
        ),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn full_ex_failure_rolls_back_the_entire_fixed_fusion_payment() {
    for count in [1_u64, 3] {
        let program = program(count);
        let mut game = position(&program, 1, usize::try_from(count).unwrap(), 0, true);
        let origin = game.clone();
        let old = question(&mut game, &program, 1);
        let answer = Answer::CostMaterials((0..count).collect());
        assert!(matches!(
            game.answer_internal_card(old.actor, old.token, answer.clone()),
            Err(sve_engine::EngineFailure::IllegalPlay(_))
        ));
        transactions::assert_restored(&game, &origin);
        let restored = exact_authority(&game);
        assert!(matches!(
            game.answer_internal_card(old.actor, old.token, answer.clone()),
            Err(sve_engine::EngineFailure::Invalid(_))
        ));
        assert_eq!(exact_authority(&game), restored);

        let fresh = question(&mut game, &program, 1);
        assert_ne!(fresh.token, old.token);
        let pending = exact_authority(&game);
        assert!(matches!(
            game.answer_internal_card(old.actor, old.token, answer.clone()),
            Err(sve_engine::EngineFailure::Invalid(_))
        ));
        assert_eq!(exact_authority(&game), pending);
        assert!(matches!(
            game.answer_internal_card(fresh.actor, fresh.token, answer),
            Err(sve_engine::EngineFailure::IllegalPlay(_))
        ));
        transactions::assert_restored(&game, &origin);
    }
}
