#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "same-name-cost",
    )
    .unwrap()
}

fn position(card: &str, field: Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = field;
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw","card":"BP01-037"},
        {"id":"draw_b","card":"BP01-037"}
    ]);
    initial
}

/// Plays the source and answers every question before the fanfare's cost materials.
fn cost_materials(game: &mut Game, card: &str) -> DecisionView {
    let mut entry = game.play_internal_card("source", PLAYER).unwrap();
    while let CardPlay::Awaiting(question) = &entry
        && matches!(question.domain, InputDomain::WardEntry { .. })
    {
        entry = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "{card} enters before its fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected the fanfare of {card}");
    };
    let mut step = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    loop {
        let CardPlay::Awaiting(question) = step else {
            panic!("the fanfare cost must be asked");
        };
        match &question.domain {
            InputDomain::OptionalCost => {
                step = game
                    .answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
                    .unwrap();
            }
            InputDomain::Targets(domain) => {
                // The printed target precedes the head payment (ECP01-004).
                let handle = domain.members[0].handle;
                step = game
                    .answer_internal_card(question.actor, question.token, Answer::Targets(vec![handle]))
                    .unwrap();
            }
            InputDomain::CostMaterials(_) => return question,
            other => panic!("unexpected question {other:?}"),
        }
    }
}

fn material_ids(game: &Game, question: &DecisionView) -> Vec<String> {
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("expected cost materials");
    };
    let state = game.projection(View::Referee).unwrap();
    let mut ids = domain
        .members
        .iter()
        .map(|member| {
            let card = member.card.clone().unwrap();
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_owned())
                .find(|id| state["objects"][id]["card"] == json!(card))
                .unwrap()
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

fn pay(game: &mut Game, question: &DecisionView) -> CardPlay {
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("expected cost materials");
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![domain.members[0].handle]),
    )
    .unwrap()
}

#[test]
fn bp21_016_returns_only_a_differently_named_beast_then_gains_storm_and_draws() {
    for (printing, twin) in [("BP21-016", "BP21-P10"), ("BP21-P10", "BP21-016")] {
        let mut game = rules_game(&position(
            printing,
            json!([
                {"id":"twin","card":twin},
                {"id":"beast","card":"BP12-076"},
                {"id":"other","card":"BP01-037"}
            ]),
        ));
        let question = cost_materials(&mut game, printing);
        assert_eq!(
            material_ids(&game, &question),
            ["beast"],
            "the same-named printing and the non-beast are not payable"
        );
        resolved(pay(&mut game, &question));
        let state = game.projection(View::Referee).unwrap();
        assert!(state["P1"]["hand"].as_array().unwrap().contains(&json!("beast")));
        assert!(state["P1"]["hand"].as_array().unwrap().contains(&json!("draw")));
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
}

#[test]
fn bp10_007_returns_a_differently_named_beast_then_searches_salvia_panther() {
    for (printing, twin) in [
        ("BP10-007", "PR-238"),
        ("BP21-PR03", "BP10-007"),
        ("PR-238", "BP21-PR03"),
    ] {
        let initial = position(
            printing,
            json!([
                {"id":"twin","card":twin},
                {"id":"beast","card":"BP12-076"}
            ]),
        );
        // サルビアパンサー (BP10-009) is not admitted yet, so the search finds nothing here.
        let mut game = rules_game(&initial);
        let question = cost_materials(&mut game, printing);
        assert_eq!(material_ids(&game, &question), ["beast"]);
        let mut step = pay(&mut game, &question);
        if let CardPlay::Awaiting(search) = &step {
            let InputDomain::Search(domain) = &search.domain else {
                panic!("expected the panther search, got {:?}", search.domain);
            };
            assert!(domain.cards.is_empty(), "only サルビアパンサー can be found");
            step = game
                .answer_internal_card(
                    search.actor,
                    search.token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![])),
                )
                .unwrap();
        }
        resolved(step);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["beast"]));
        assert!(state["P1"]["field"].as_array().unwrap().contains(&json!("twin")));
    }
}

#[test]
fn ecp01_004_returns_a_differently_named_umamusume_card_to_deal_five() {
    for (printing, twin) in [
        ("ECP01-004", "ECP01-SL04"),
        ("ECP01-SL04", "ECP01-SP04"),
        ("ECP01-SP04", "ECP01-SSP04"),
        ("ECP01-SSP04", "ECP01-004"),
    ] {
        let mut initial = position(
            printing,
            json!([
                {"id":"twin","card":twin},
                {"id":"uma","card":"ECP01-053"}
            ]),
        );
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = rules_game(&initial);
        let question = cost_materials(&mut game, printing);
        // Two Umamusume on the field paid 9 - 4 = 5 PP.
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            5_u8
        );
        assert_eq!(material_ids(&game, &question), ["uma"]);
        resolved(pay(&mut game, &question));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
        assert!(state["P1"]["hand"].as_array().unwrap().contains(&json!("uma")));
    }
}

fn main_phase(field: Value) -> Game {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = field;
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw","card":"BP01-037"},
        {"id":"draw_b","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy_draw","card":"BP01-037"}]);
    let mut game = rules_game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the opponent turn ends"
    );
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        // The main-phase trigger waits for its controller before anyone may Quick.
        if game.internal_pending_choice().unwrap().is_some() {
            break;
        }
        let Some(quick) = game.internal_quick_choice().unwrap() else {
            break;
        };
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
    }
    game
}

fn resolve_pending(game: &mut Game) {
    while let Some(pending) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected the main-phase trigger");
        };
        resolved(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
    }
}

#[test]
fn cp01_076_returns_itself_unless_a_differently_named_mejiro_follower_stays() {
    for (printing, twin) in [
        ("CP01-076", "CP01-P58"),
        ("CP01-P58", "CSD01-028"),
        ("CSD01-028", "CP01-076"),
    ] {
        let mut alone =
            main_phase(json!([{"id":"source","card":printing},{"id":"twin","card":twin}]));
        resolve_pending(&mut alone);
        let state = alone.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["hand"].as_array().unwrap().contains(&json!("source")),
            "a same-named Mejiro Ardan does not keep {printing} on the field"
        );
        let mut kept = main_phase(json!([
            {"id":"source","card":printing},
            {"id":"mejiro","card":"CP01-072"}
        ]));
        resolve_pending(&mut kept);
        let kept_state = kept.projection(View::Referee).unwrap();
        assert!(
            kept_state["P1"]["field"].as_array().unwrap().contains(&json!("source")),
            "another Mejiro follower keeps {printing} on the field"
        );
    }
}
