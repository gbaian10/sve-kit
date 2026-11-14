#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::DistributionMember;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-source-bottom-cost",
    )
    .unwrap()
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"BP01-037"},
        {"id":"next","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

/// Answers every question, picking target candidates by printed card number.
fn drive(game: &mut Game, mut step: Option<CardPlay>, picks: &[&str]) {
    for _ in 0_u8..64_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let pick = |members: &[DistributionMember]| {
                members
                    .iter()
                    .filter(|m| m.card.as_deref().is_some_and(|c| picks.contains(&c)))
                    .take(1)
                    .map(|m| m.handle)
                    .collect::<Vec<_>>()
            };
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(pick(&d.members)),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::Quick { .. } => Answer::Quick(None),
                _ => panic!("unexpected decision: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected a pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("scenario did not settle");
}

fn ability(line: u32) -> AbilityId {
    selection_with_rules().current["BP07-012"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp07_012_acts_and_bottoms_itself_to_count_an_ex_fairy_tale_follower() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP07-012"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"stored","card":"BP07-098"}]);
    let mut game = loaded(&initial);
    let step = game
        .activate_internal_card("source", ability(2), PLAYER)
        .unwrap();
    drive(&mut game, Some(step), &["BP07-098"]);
    let view = state(&game);
    assert_eq!(
        view["P1"]["deck"],
        json!(["top", "next", "source"]),
        "The source pays by going to the bottom of its owner's deck"
    );
    assert_eq!(view["P1"]["field"], json!([]));
    assert_eq!(view["objects"]["stored"]["counters"]["童話カウンター"], 2_u8);
}

#[test]
fn bp07_012_may_choose_itself_but_its_counters_do_not_follow_it_to_the_deck() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP07-012"},
        {"id":"ally","card":"BP07-098"}
    ]);
    let mut game = loaded(&initial);
    let step = game
        .activate_internal_card("source", ability(2), PLAYER)
        .unwrap();
    let CardPlay::Awaiting(q) = &step else {
        panic!("the ability chooses its target before paying");
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("expected the target question");
    };
    let mut candidates: Vec<_> = domain.members.iter().filter_map(|m| m.card.clone()).collect();
    candidates.sort();
    assert_eq!(
        candidates,
        ["BP07-012", "BP07-098"],
        "Fairy-tale followers on the field, including the source, are legal targets"
    );
    drive(&mut game, Some(step), &["BP07-012"]);
    let view = state(&game);
    assert_eq!(view["P1"]["deck"], json!(["top", "next", "source"]));
    assert!(
        view["objects"]["source"]["counters"]["童話カウンター"].is_null(),
        "The bottomed source is a new object and keeps no counters"
    );
    assert!(view["objects"]["ally"]["counters"]["童話カウンター"].is_null());
}

#[test]
fn bp07_012_cannot_pay_while_acted_or_from_ex() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP07-012","state":{"acted":true}}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"stored","card":"BP07-098"}]);
    let mut game = loaded(&initial);
    let refused = match game.activate_internal_card("source", ability(2), PLAYER) {
        Err(_) => true,
        Ok(CardPlay::Awaiting(q)) => {
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("expected the target question");
            };
            let answer = Answer::Targets(vec![domain.members[0].handle]);
            game.answer_internal_card(q.actor, q.token, answer).is_err()
        }
        Ok(_) => false,
    };
    assert!(refused, "An acted source cannot act again as its cost");
    let view = state(&game);
    assert_eq!(view["P1"]["field"], json!(["source"]));
    assert!(view["objects"]["stored"]["counters"]["童話カウンター"].is_null());

    let mut in_ex = position();
    in_ex["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"source","card":"BP07-012"},
        {"id":"stored","card":"BP07-098"}
    ]);
    let mut from_ex = loaded(&in_ex);
    assert!(
        from_ex
            .activate_internal_card("source", ability(2), PLAYER)
            .is_err(),
        "The field ability is not usable from EX"
    );
}

#[test]
fn bp07_012_fanfare_deals_one_damage_when_played_from_hand() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP07-012"}]);
    let mut game = loaded(&initial);
    let step = game.play_internal_card("source", PLAYER).unwrap();
    drive(&mut game, Some(step), &["BP01-037"]);
    assert_eq!(state(&game)["objects"]["enemy"]["damage"], 1_u8);
}
