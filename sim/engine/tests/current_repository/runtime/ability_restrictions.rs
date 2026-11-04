#![expect(
    clippy::panic,
    reason = "Unexpected formal questions invalidate these printed activation scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn position(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "activation-prohibition",
    )
    .unwrap()
}
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed question")
    };
    q
}
fn ability(number: &str, line: u32) -> AbilityId {
    selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line && a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}
fn pending(game: &mut Game, number: &str) -> CardPlay {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending")
    };
    assert_eq!(options.len(), 1, "one printed occurrence must be pending");
    assert_eq!(
        options[0].card, number,
        "the occurrence must use the effective printing"
    );
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}
fn target(game: &mut Game, q: &DecisionView) -> CardPlay {
    let InputDomain::Targets(d) = &q.domain else {
        panic!("target before cost")
    };
    let member = d
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some("BP01-037"))
        .unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![member.handle]))
        .unwrap()
}
fn pay(game: &mut Game, q: &DecisionView, seat: u32) -> Activation {
    let InputDomain::CostMaterials(d) = &q.domain else {
        panic!("shikigami cost")
    };
    let member = d
        .members
        .iter()
        .find(|m| m.player.seat == seat && m.card.as_deref() == Some("BP06-T02"))
        .unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![member.handle]))
            .unwrap(),
    )
}
fn lastword(game: &mut Game, seat: u32) {
    let discard = question(pending(game, "BP06-T02"));
    assert_eq!(
        discard.actor.seat, seat,
        "the sacrificed token owner answers its lastword"
    );
    let InputDomain::Targets(d) = &discard.domain else {
        panic!("draw then discard")
    };
    assert_eq!(
        d.members.len(),
        1,
        "the token drew its only discard candidate"
    );
    resolved(
        game.answer_internal_card(
            discard.actor,
            discard.token,
            Answer::Targets(vec![d.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the token lastword must finish before the next action"
    );
}

#[test]
fn curse_enchanter_all_printings_create_shikigami_and_cannot_reactivate_after_evolution() {
    for number in ["BP06-039", "PR-159"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP06-040"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"hp":8_u8}},
            {"id":"opponent-material","card":"BP06-T02"}
        ]);
        initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-168"}]);
        let mut game = position(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolved(pending(&mut game, number));
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
        let own_token = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .find(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP06-T02")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let declaration = question(
            game.activate_internal_card("source", ability(number, 3), PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["field"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let materials = question(target(&mut game, &declaration));
        let paid = pay(&mut game, &materials, 2);
        let paid_state = game.projection(View::Referee).unwrap();
        assert_eq!(paid_state["objects"]["enemy"]["hp"], 4_u8);
        assert_eq!(paid_state["P2"]["field"], json!(["enemy"]));
        assert!(
            paid_state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!(own_token))
        );
        assert!(
            !paid
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
        lastword(&mut game, 2);
        let before = game.projection(View::Referee).unwrap();
        game.activate_internal_card("source", ability(number, 3), PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let evolve_target = question(pending(&mut game, "BP06-040"));
        resolved(target(&mut game, &evolve_target));
        game.resume_internal_card().unwrap();
        let evolved_state = game.projection(View::Referee).unwrap();
        assert_eq!(evolved_state["objects"]["enemy"]["hp"], 2_u8);
        game.activate_internal_card("source", ability("BP06-040", 2), PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), evolved_state);
    }
}

#[test]
fn evolved_curse_enchanter_counts_the_union_once_and_locks_its_remaining_material() {
    for unique in [6_u8, 7_u8] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"BP06-039"},
            {"id":"material-one","card":"BP06-T02"},
            {"id":"material-two","card":"BP06-T02"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP06-040"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"overlap1","card":"BP06-042"},{"id":"overlap2","card":"BP06-042"},{"id":"overlap3","card":"BP06-042"},
            {"id":"spell1","card":"BP01-168"},{"id":"spell2","card":"BP01-168"},
            {"id":"follower1","card":"BP06-048"}
        ]);
        if unique == 7_u8 {
            initial["players"]["P1"]["zones"]["cemetery"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"follower2","card":"BP06-048"}));
        }
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-168"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8}}]);
        let mut game = position(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let evolution_target = question(pending(&mut game, "BP06-040"));
        resolved(target(&mut game, &evolution_target));
        game.resume_internal_card().unwrap();
        let expected = if unique == 7_u8 { 6_u8 } else { 8_u8 };
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            expected
        );
        let declaration = question(
            game.activate_internal_card("source", ability("BP06-040", 2), PlayerId { seat: 1 })
                .unwrap(),
        );
        let materials = question(target(&mut game, &declaration));
        pay(&mut game, &materials, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            expected - 4
        );
        lastword(&mut game, 1);
        let before = game.projection(View::Referee).unwrap();
        assert!(
            before["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("material-two"))
        );
        game.activate_internal_card("source", ability("BP06-040", 2), PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}
