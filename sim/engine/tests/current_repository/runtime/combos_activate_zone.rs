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
        "combos-activate-zone",
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
    initial
}

/// Answers every question, picking target or material candidates by printed card number.
fn drive(game: &mut Game, mut step: Option<CardPlay>, picks: &[&str]) {
    for _ in 0_u8..64_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let pick = |members: &[DistributionMember], maximum: usize| {
                members
                    .iter()
                    .filter(|m| m.card.as_deref().is_some_and(|c| picks.contains(&c)))
                    .take(maximum)
                    .map(|m| m.handle)
                    .collect::<Vec<_>>()
            };
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(pick(&d.members, 1)),
                InputDomain::CostMaterials(d) => Answer::CostMaterials(pick(
                    &d.members,
                    usize::try_from(d.cardinality.minimum.0).unwrap(),
                )),
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

/// Activates the source's printed ability on `line`.
fn activate(game: &mut Game, card: &str, line: u32, picks: &[&str]) {
    let program = &selection_with_rules().current[card].program;
    let ability = program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", ability, PLAYER).unwrap();
    drive(game, Some(step), picks);
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp11_004_moves_itself_from_hand_then_grows_in_ex() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP11-004"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"stored","card":"BP01-048"}]);
    let mut game = loaded(&initial);
    activate(&mut game, "BP11-004", 2, &[]);
    let moved = state(&game);
    assert_eq!(moved["P1"]["pp"]["current"], 8_u8);
    let ex = moved["P1"]["ex"].as_array().unwrap();
    assert!(ex.contains(&json!("source")) && ex.contains(&json!("top")));
    activate(&mut game, "BP11-004", 3, &["BP01-048", "BP01-037"]);
    let grown = state(&game);
    assert_eq!(grown["P1"]["ex"], json!(["source"]));
    assert_eq!(grown["objects"]["source"]["power"], 2_u8);
    assert_eq!(grown["P1"]["hand"], json!(["next"]));
    let banished = grown["P1"]["banish"].as_array().unwrap();
    assert!(banished.contains(&json!("stored")) && banished.contains(&json!("top")));
}

#[test]
fn bp12_054_sends_itself_from_ex_to_deal_six() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP12-054"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8}}]);
    let mut game = loaded(&initial);
    activate(&mut game, "BP12-054", 4, &["BP01-037"]);
    let view = state(&game);
    assert_eq!(view["objects"]["enemy"]["damage"], 6_u8);
    assert_eq!(view["P1"]["cemetery"], json!(["source"]));
    assert_eq!(view["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn bp12_095_banishes_itself_from_ex_to_heal_and_draw() {
    let mut initial = position();
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP12-095"}]);
    let mut game = loaded(&initial);
    activate(&mut game, "BP12-095", 5, &[]);
    let view = state(&game);
    assert_eq!(view["P1"]["leader"]["life"], 16_u8);
    assert_eq!(view["P1"]["hand"], json!(["top"]));
    assert_eq!(view["P1"]["banish"], json!(["source"]));
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp17_099_enters_acted_from_ex_with_five_machines() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"source","card":"BP17-099"},
        {"id":"m1","card":"BP07-035"},
        {"id":"m2","card":"BP07-042"},
        {"id":"m3","card":"BP07-044"},
        {"id":"m4","card":"BP07-046"}
    ]);
    let mut game = loaded(&initial);
    activate(&mut game, "BP17-099", 2, &[]);
    let view = state(&game);
    assert_eq!(view["P1"]["field"], json!(["source"]));
    assert_eq!(view["objects"]["source"]["acted"], true);
    assert_eq!(view["P1"]["pp"]["current"], 10_u8);
}

#[test]
fn bp17_099_hand_look_keeps_an_untaken_card_on_top() {
    // Q2268: a card not added to the hand stays unrevealed on top of the deck.
    for (top, pick) in [
        ("BP07-035", Some("BP07-035")),
        ("BP07-035", None),
        ("BP01-037", None),
    ] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP17-099"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"top","card":top}, {"id":"next","card":"BP01-037"}]);
        let mut game = loaded(&initial);
        activate(&mut game, "BP17-099", 3, pick.as_slice());
        let view = state(&game);
        assert_eq!(view["P1"]["ex"], json!(["source"]));
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        if pick.is_some() {
            assert_eq!(view["P1"]["hand"], json!(["top"]));
            assert_eq!(view["P1"]["deck"], json!(["next"]));
        } else {
            assert_eq!(view["P1"]["hand"], json!([]));
            assert_eq!(view["P1"]["deck"], json!(["top", "next"]), "{top}");
            let public = game.projection(View::P2).unwrap();
            assert!(
                !public["known_cards"]
                    .as_array()
                    .is_some_and(|cards| cards.iter().any(|card| card["id"] == "top")),
                "the untaken card stays private"
            );
        }
    }
}
