#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;
use core::iter::once;
use sve_engine::game::internal::DistributionMember;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-paid-count-discard",
    )
    .unwrap()
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"BP01-037"},
        {"id":"d2","card":"BP01-037"},
        {"id":"d3","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

/// Answers every question by printed card number; each `materials` entry pays one fairy.
fn drive(game: &mut Game, mut step: Option<CardPlay>, materials: &[&str], discards: &[&str]) {
    for _ in 0_u8..64_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let pick = |members: &[DistributionMember], cards: &[&str], limit: usize| {
                members
                    .iter()
                    .filter(|m| m.card.as_deref().is_some_and(|c| cards.contains(&c)))
                    .take(limit)
                    .map(|m| m.handle)
                    .collect::<Vec<_>>()
            };
            let answer = match &q.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::CostMaterials(d) => {
                    assert!(
                        d.members
                            .iter()
                            .all(|m| m.card.as_deref() == Some("BP01-013")),
                        "Only fairy cards in EX pay the cost"
                    );
                    Answer::CostMaterials(pick(&d.members, materials, materials.len()))
                }
                InputDomain::Targets(d) => {
                    let exact = usize::try_from(d.cardinality.minimum.0).unwrap();
                    Answer::Targets(pick(&d.members, discards, exact))
                }
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

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn sorted(zone: &Value) -> Vec<String> {
    let mut ids: Vec<_> = zone
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    ids
}

fn hand_position() -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP22-002"},
        {"id":"kept","card":"BP01-014"},
        {"id":"spare","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"f1","card":"BP01-013"},
        {"id":"f2","card":"BP01-013"},
        {"id":"other","card":"BP01-037"}
    ]);
    initial
}

#[test]
fn bp22_002_draws_and_discards_as_many_as_it_banished() {
    let mut game = loaded(&hand_position());
    let step = game.play_internal_card("source", PLAYER).unwrap();
    drive(&mut game, Some(step), &["BP01-013", "BP01-013"], &["BP01-037"]);
    let view = state(&game);
    assert_eq!(sorted(&view["P1"]["banish"]), ["f1", "f2"]);
    assert_eq!(view["P1"]["ex"], json!(["other"]));
    assert_eq!(view["P1"]["deck"], json!(["d3"]), "Two banished fairies draw two");
    assert_eq!(
        sorted(&view["P1"]["cemetery"]).len(),
        2,
        "Two banished fairies discard two"
    );
    assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 2);
    assert!(view["P1"]["hand"].as_array().unwrap().contains(&json!("kept")));
}

#[test]
fn bp22_002_counts_the_fairies_paid_not_the_fairies_available() {
    let mut game = loaded(&hand_position());
    let step = game.play_internal_card("source", PLAYER).unwrap();
    drive(&mut game, Some(step), &["BP01-013"], &["BP01-037"]);
    let view = state(&game);
    assert_eq!(view["P1"]["banish"].as_array().unwrap().len(), 1);
    assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 2);
    assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 2, "One fairy draws one");
    assert_eq!(view["P1"]["cemetery"].as_array().unwrap().len(), 1, "and discards one");
}

#[test]
fn bp22_002_draws_and_discards_nothing_for_zero_fairies() {
    let mut game = loaded(&hand_position());
    let step = game.play_internal_card("source", PLAYER).unwrap();
    // Paying zero fairies is legal; a discard question here would panic for lack of picks.
    drive(&mut game, Some(step), &[], &[]);
    let view = state(&game);
    assert_eq!(view["P1"]["banish"], json!([]));
    assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 3);
    assert_eq!(sorted(&view["P1"]["hand"]), ["kept", "spare"]);
    assert_eq!(view["P1"]["cemetery"], json!([]));
}

fn activation() -> AbilityId {
    selection_with_rules().current["BP22-002"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 3)
        .unwrap()
        .node
        .id
}

fn field_position(elves: usize) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-002"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
        (0..elves)
            .map(|i| json!({"id":format!("elf{i}"),"card":"BP01-013"}))
            .chain([json!({"id":"royal","card":"BP01-037"})])
            .collect(),
    );
    initial
}

#[test]
fn bp22_002_hits_every_opponent_leader_and_follower_with_twenty_elf_cards() {
    let mut game = loaded(&field_position(20));
    let step = game
        .activate_internal_card("source", activation(), PLAYER)
        .unwrap();
    drive(&mut game, Some(step), &[], &[]);
    let view = state(&game);
    assert_eq!(view["P2"]["leader"]["life"], 14_u8);
    assert_eq!(view["P2"]["field"], json!([]));
    assert_eq!(view["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn bp22_002_cannot_activate_with_nineteen_elf_cards() {
    let mut game = loaded(&field_position(19));
    assert!(
        game.activate_internal_card("source", activation(), PLAYER)
            .is_err(),
        "A non-Elf card does not count toward the twenty"
    );
}

/// BP22-002 with its fanfare body replaced by discards whose count is read on resolution.
fn synthetic(fanfare: &Value) -> SelectedEffects {
    let (snapshot, keywords, mut input) = inputs().clone();
    let authors = input["authors"].as_array_mut().unwrap();
    let author = authors
        .iter_mut()
        .find(|author| author["input"]["card_no"] == "BP22-002")
        .unwrap();
    let faces = author["input"]["tree"]["cards"]["BP22-002"]["faces"]
        .as_object_mut()
        .unwrap();
    let face = faces.values_mut().next().unwrap();
    face["abilities"][1]["fanfare"]["do"] = fanfare.clone();
    repository::select(&snapshot, &keywords, &input)
        .unwrap_or_else(|error| panic!("synthetic selection: {error:?}"))
}

fn synthetic_game(fanfare: &Value, hand: usize, cemetery: usize) -> Game {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = Value::Array(
        once(json!({"id":"source","card":"BP22-002"}))
            .chain((0..hand).map(|i| json!({"id":format!("h{i}"),"card":"BP01-037"})))
            .collect(),
    );
    initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
        (0..cemetery)
            .map(|i| json!({"id":format!("c{i}"),"card":"BP01-014"}))
            .collect(),
    );
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"o1","card":"BP01-037"},
        {"id":"o2","card":"BP01-037"}
    ]);
    let mut game = Game::from_selected_effects(
        synthetic(fanfare),
        &initial,
        &Value::Null,
        &Value::Null,
        "combos-paid-count-discard",
    )
    .unwrap();
    let step = game.play_internal_card("source", PLAYER).unwrap();
    drive(&mut game, Some(step), &[], &["BP01-037"]);
    game
}

fn cemetery_count() -> Value {
    json!({"count":{"who":"self","zone":"cemetery"}})
}

fn zone_len(view: &Value, seat: &str, zone: &str) -> usize {
    view[seat][zone].as_array().unwrap().len()
}

#[test]
fn counted_discard_reads_its_value_after_the_previous_instruction() {
    let game = synthetic_game(
        &json!([
            {"discard":{"who":"self","n":1_u8}},
            {"discard":{"who":"self","n":cemetery_count()}}
        ]),
        4,
        0,
    );
    let view = state(&game);
    assert_eq!(zone_len(&view, "P1", "hand"), 2, "The second discard reads one cemetery card");
    assert_eq!(zone_len(&view, "P1", "cemetery"), 2);
}

#[test]
fn counted_discard_takes_the_whole_hand_when_it_asks_for_more() {
    let game = synthetic_game(&json!([{"discard":{"who":"self","n":cemetery_count()}}]), 2, 3);
    let view = state(&game);
    assert_eq!(zone_len(&view, "P1", "hand"), 0);
    assert_eq!(zone_len(&view, "P1", "cemetery"), 5);

    let second = synthetic_game(&json!([{"discard":{"who":"self","n":cemetery_count()}}]), 0, 2);
    let after = state(&second);
    assert_eq!(zone_len(&after, "P1", "cemetery"), 2, "An empty hand discards nothing");
}

#[test]
fn counted_discard_may_be_random_or_the_opponents_choice() {
    let game = synthetic_game(
        &json!([{"discard":{"who":"self","n":cemetery_count(),"random":true}}]),
        3,
        2,
    );
    let view = state(&game);
    assert_eq!(zone_len(&view, "P1", "hand"), 1, "Two random cards leave without a question");

    let second = synthetic_game(
        &json!([{"discard":{"who":"opp","n":cemetery_count()}}]),
        0,
        1,
    );
    let after = state(&second);
    assert_eq!(zone_len(&after, "P2", "hand"), 1);
    assert_eq!(zone_len(&after, "P2", "cemetery"), 1);
}

#[test]
fn counted_discard_names_the_cards_it_actually_discarded() {
    let game = synthetic_game(
        &json!([
            {"discard":{"who":"self","n":cemetery_count(),"random":false,"result":"tossed"}},
            {"draw":{"n":{"count_of":"tossed"}}}
        ]),
        3,
        2,
    );
    let view = state(&game);
    assert_eq!(zone_len(&view, "P1", "hand"), 3, "Two discarded, two drawn");
    assert_eq!(zone_len(&view, "P1", "deck"), 1);
}
