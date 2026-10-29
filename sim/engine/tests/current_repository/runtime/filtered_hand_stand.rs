#![expect(
    clippy::panic,
    reason = "Unexpected fixture questions invalidate the printed card path."
)]
use super::*;

fn pending(game: &mut Game, printing: &str) -> CardPlay {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let occurrence = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &occurrence.domain else {
        panic!("printed trigger");
    };
    assert_eq!(options.len(), 1, "exactly the printed trigger is pending");
    assert_eq!(
        options[0].card, printing,
        "the actual printing supplies the ability"
    );
    game.answer_internal_card(
        occurrence.actor,
        occurrence.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn pay_material(game: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(materials) = play else {
        panic!("hand materials after targets");
    };
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("private cost materials");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the printed high-cost Dragon is eligible"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("BP01-097"),
        "effective cost zero does not exclude original cost seven"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "opponents cannot see private candidates"
    );
    let paid = game
        .answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap();
    resolved(paid);
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn end(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let window = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
        .unwrap();
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP22-068"}, {"id":"cost","card":"BP01-097","state":{"cost":0_u8}},
        {"id":"low","card":"BP01-037","state":{"cost":9_u8}}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-069"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"p1-draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"p2-draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"standing","card":"BP01-037"},
        {"id":"already-acted","card":"BP01-037","state":{"acted":true}},
        {"id":"not-targeted","card":"BP01-037","state":{"acted":true}}]);
    initial
}

#[test]
fn bp22_068_pays_a_printed_seven_dragon_and_069_evolves_after_freezing_targets() {
    let mut game = game(&position());
    game.play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let fanfare = pending(&mut game, "BP22-068");
    let CardPlay::Awaiting(declare) = fanfare else {
        panic!("head payment declaration");
    };
    assert_eq!(declare.domain, InputDomain::OptionalCost);
    let accepted = game
        .answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(true))
        .unwrap();
    let CardPlay::Awaiting(targets) = accepted else {
        panic!("public targets");
    };
    let InputDomain::Targets(candidates) = &targets.domain else {
        panic!("targets precede hand cost");
    };
    assert_eq!(candidates.members.len(), 3);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["cost", "low"])
    );
    let selected = game
        .answer_internal_card(
            targets.actor,
            targets.token,
            Answer::Targets(
                candidates
                    .members
                    .iter()
                    .take(2)
                    .map(|member| member.handle)
                    .collect(),
            ),
        )
        .unwrap();
    pay_material(&mut game, selected);
    let paid_view = game.projection(View::Referee).unwrap();
    assert_eq!(paid_view["P1"]["cemetery"], json!(["cost"]));
    assert_eq!(paid_view["objects"]["standing"]["acted"], true);
    assert_eq!(paid_view["objects"]["already-acted"]["acted"], true);
    end(&mut game);
    let start_view = game.projection(View::Referee).unwrap();
    assert_eq!(start_view["objects"]["standing"]["acted"], true);
    assert_eq!(start_view["objects"]["already-acted"]["acted"], true);
    assert_eq!(start_view["objects"]["not-targeted"]["acted"], false);
    end(&mut game);
    let evolved = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(evolved.outcome, "resolved");
    let on_evolve = pending(&mut game, "BP22-069");
    let CardPlay::Awaiting(damage_target) = on_evolve else {
        panic!("evolved target");
    };
    let InputDomain::Targets(damage_candidates) = &damage_target.domain else {
        panic!("one damage target");
    };
    let damaged = game
        .answer_internal_card(
            damage_target.actor,
            damage_target.token,
            Answer::Targets(vec![damage_candidates.members[0].handle]),
        )
        .unwrap();
    resolved(damaged);
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["standing"])
    );
}
