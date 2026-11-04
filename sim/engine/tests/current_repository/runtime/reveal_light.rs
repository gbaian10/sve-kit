#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these printed reveal scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn position(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "reveal-light",
    )
    .unwrap()
}
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed question")
    };
    q
}
fn pending(game: &mut Game, number: &str) -> CardPlay {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending")
    };
    assert_eq!(options.len(), 1, "one printed fanfare must be pending");
    assert_eq!(
        options[0].card, number,
        "pending source must match the printing"
    );
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}
fn accept(game: &mut Game, play: CardPlay) -> DecisionView {
    let q = question(play);
    assert_eq!(
        q.domain,
        InputDomain::OptionalCost,
        "declare the head payment before selecting its materials"
    );
    question(
        game.answer_internal_card(q.actor, q.token, Answer::OptionalCost(true))
            .unwrap(),
    )
}
fn reveal(game: &mut Game, q: &DecisionView, cards: &[&str]) -> CardPlay {
    let InputDomain::CostMaterials(d) = &q.domain else {
        panic!("private reveal materials")
    };
    assert_eq!(
        d.members
            .iter()
            .map(|m| m.card.as_deref().unwrap())
            .collect::<Vec<_>>(),
        cards,
        "only cards matching the printed payment filter are eligible"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot answer a private material question"
    );
    game.answer_internal_card(
        q.actor,
        q.token,
        Answer::CostMaterials(d.members.iter().map(|m| m.handle).collect()),
    )
    .unwrap()
}

#[test]
fn mirubiru_all_printings_reveal_a_royal_paladin_card_to_draw() {
    for number in ["CP03-031", "CP03-P19"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"paladin","card":"CP03-038"},{"id":"wrong","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"CP03-031"}]);
        let mut game = position(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let head = pending(&mut game, number);
        let materials = accept(&mut game, head);
        let result = resolved(reveal(&mut game, &materials, &["CP03-038"]));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["paladin", "wrong", "drawn"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        assert!(game.projection(View::P2).unwrap()["objects"]["paladin"].is_null());
    }
}

#[test]
fn evolved_devouring_serpent_all_printings_stand_then_act_and_reveal_for_damage() {
    for number in ["BP18-083", "BP18-P33"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP18-082","state":{"acted":true}}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"first","card":"BP18-082"},{"id":"second","card":"BP18-082"},{"id":"wrong","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":8_u8}}]);
        let mut game = position(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":5_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        resolved(pending(&mut game, number));
        game.resume_internal_card().unwrap();
        let evolved = game.projection(View::Referee).unwrap();
        assert_eq!(evolved["objects"]["source"]["acted"], false);
        assert!(
            evolved["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
        let definition = selection_with_rules().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.source.line == 3 && a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let target = question(
            game.activate_internal_card("source", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        let materials = question(
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let result = resolved(reveal(&mut game, &materials, &["BP18-082", "BP18-082"]));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["enemy"]["hp"], 4_u8);
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
        assert_eq!(state["P1"]["hand"], json!(["first", "second", "wrong"]));
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        assert!(game.projection(View::P2).unwrap()["objects"]["first"].is_null());
    }
}

#[test]
fn digging_crow_all_printings_reveal_two_original_two_cost_cards_to_heal() {
    for number in ["BP18-086", "BP18-P35"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(17_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"first","card":"BP18-082"},{"id":"second","card":"BP18-086"},
            {"id":"wrong","card":"BP01-037","state":{"cost":2_u8}}
        ]);
        let mut game = position(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let head = pending(&mut game, number);
        let materials = accept(&mut game, head);
        let result = resolved(reveal(&mut game, &materials, &["BP18-082", "BP18-086"]));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            19_u8
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["first", "second", "wrong"])
        );
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        assert!(game.projection(View::P2).unwrap()["objects"]["second"].is_null());
    }
}

#[test]
fn julius_all_printings_choose_three_modes_before_targets_and_reveal_payment() {
    for number in ["BP21-022", "BP21-SL08", "BP21-U02"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"first","card":"BP21-022"},{"id":"second","card":"BP21-022"}
        ]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..5_u8)
                .map(|n| json!({"id":format!("cem{n}"),"card":"BP21-022"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"d1","card":"BP01-168"},{"id":"d2","card":"BP01-168"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = position(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let head = pending(&mut game, number);
        let modes = accept(&mut game, head);
        assert!(game.projection(View::P2).unwrap()["objects"]["first"].is_null());
        let InputDomain::Modes { count, .. } = modes.domain else {
            panic!("modes before targets and cost")
        };
        assert_eq!(count, Nat(3));
        let target = question(
            game.answer_internal_card(
                modes.actor,
                modes.token,
                Answer::Modes(vec![ModeId(1), ModeId(2), ModeId(3)]),
            )
            .unwrap(),
        );
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        let materials = question(
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let discard = question(reveal(&mut game, &materials, &["BP21-022", "BP21-022"]));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(state["P2"]["leader"]["life"], 17_u8);
        assert_eq!(
            game.projection(View::P2).unwrap()["objects"]["first"]["card"],
            "BP21-022"
        );
        let InputDomain::Targets(d) = &discard.domain else {
            panic!("draw then discard")
        };
        assert_eq!(d.members.len(), 4);
        let result = resolved(
            game.answer_internal_card(
                discard.actor,
                discard.token,
                Answer::Targets(vec![d.members.last().unwrap().handle]),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["first", "second", "d1"])
        );
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|f| matches!(f, Fact::Discard(_)))
                .count(),
            1
        );
        assert!(game.projection(View::P2).unwrap()["objects"]["first"].is_null());
    }
}
