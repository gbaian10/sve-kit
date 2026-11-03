#![expect(
    clippy::panic,
    reason = "A different question invalidates the printed payment scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed payment question");
    };
    question
}

fn fanfare(game: &mut Game, number: &str) -> DecisionView {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed fanfare");
    };
    assert_eq!(options.len(), 1, "one printed fanfare must be pending");
    assert_eq!(options[0].card, number, "pending source must match the printing");
    let declare = question(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(declare.domain, InputDomain::OptionalCost, "the head cost must be declared before selecting materials");
    question(
        game.answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(true))
            .unwrap(),
    )
}

fn reveal(game: &mut Game, question: &DecisionView, expected: &[&str]) -> Activation {
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("reveal material choice");
    };
    assert_eq!(
        domain
            .members
            .iter()
            .map(|member| member.card.as_deref().unwrap())
            .collect::<Vec<_>>(),
        expected,
        "only matching own hand cards may be payment candidates"
    );
    assert!(game.internal_decision(View::P2).is_none(), "the opponent must not see the private payment question");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect()),
        )
        .unwrap(),
    )
}

#[test]
fn cp03_070_reveals_an_overlord_follower_to_draw_without_discarding_it() {
    for number in ["CP03-070", "PR-279"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"overlord","card":"CP03-063"},
            {"id":"other","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "reveal-vanguard",
        )
        .unwrap();
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let material = fanfare(&mut game, number);
        let result = reveal(&mut game, &material, &["CP03-063"]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["overlord", "other", "drawn"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        assert!(game.projection(View::P2).unwrap()["objects"]["overlord"].is_null());
    }
}

#[test]
fn bp06_037_reveals_an_academy_follower_for_damage_and_discounts_its_later_play() {
    for number in ["BP06-037", "BP06-SL08", "DSD01a-004", "PR-364"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"academy","card":"BP06-037"},
            {"id":"spell","card":"BP01-168"}
        ]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":8_i32}}]);
        let mut game = game(&initial);
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let target = fanfare(&mut game, number);
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("damage target before materials");
        };
        assert_eq!(domain.members.len(), 1);
        assert!(game.projection(View::P2).unwrap()["objects"]["academy"].is_null());
        let materials = question(
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        reveal(&mut game, &materials, &["BP06-037"]);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 6_i32);
        assert_eq!(state["P1"]["hand"], json!(["academy", "spell"]));
        assert_eq!(state["P1"]["pp"]["current"], 7_i32);
        game.play_internal_card("academy", PlayerId { seat: 1 })
            .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            5_i32
        );
    }
}

#[test]
fn bp18_082_acts_and_reveals_two_distinct_original_cost_two_cards_before_damage() {
    for number in ["BP18-082", "BP18-P32"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"a","card":"BP18-082","state":{"cost":0_i32}},
            {"id":"b","card":"BP18-082","state":{"cost":9_i32}},
            {"id":"wrong","card":"BP01-037","state":{"cost":2_i32}}
        ]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":8_i32}}]);
        let mut game = game(&initial);
        let definition = selection().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.source.line == 2 && ability.node.kind == AbilityKind::Activated)
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
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            false
        );
        let InputDomain::CostMaterials(domain) = &materials.domain else {
            panic!("two materials");
        };
        assert_eq!(domain.cardinality.minimum, Nat(2));
        let result = reveal(&mut game, &materials, &["BP18-082", "BP18-082"]);
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["P1"]["pp"]["current"], 10_i32);
        assert_eq!(state["P1"]["hand"], json!(["a", "b", "wrong"]));
        assert_eq!(state["objects"]["enemy"]["hp"], 4_i32);
        assert!(game.projection(View::P2).unwrap()["objects"]["a"].is_null());
    }
}
