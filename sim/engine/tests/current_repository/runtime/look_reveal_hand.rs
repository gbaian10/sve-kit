#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn deck(cards: &[(&str, &str)]) -> Value {
    Value::Array(
        cards
            .iter()
            .map(|(id, card)| json!({"id":id,"card":card}))
            .collect(),
    )
}

fn in_hand(card: &str, library: &[(&str, &str)]) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = deck(library);
    initial
}

fn on_field(card: &str, library: &[(&str, &str)]) -> Value {
    let mut initial = in_hand(card, library);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

// Collaboration cards need the registered title rules.
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "look-reveal-hand",
    )
    .unwrap()
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(take(&domain.cardinality, domain.cards.len()))
                    .map(|card| card.handle)
                    .collect(),
            )),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().map(|card| card.handle).collect())
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn triggers(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step);
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step);
    triggers(game);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers(game);
}

fn printed_activation(card: &str) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id
}

fn activate(game: &mut Game, card: &str) {
    let step = game
        .activate_internal_card("source", printed_activation(card), actor())
        .unwrap();
    finish(game, step);
    triggers(game);
}

/// The chosen card is in hand and the opponent saw it; the others stay hidden.
fn assert_revealed(game: &Game, shown: &[&str], hidden: &[&str]) {
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    for id in shown {
        assert!(
            known.iter().any(|card| card["id"] == *id),
            "{id} is revealed to the opponent"
        );
    }
    for id in hidden {
        assert!(
            !known.iter().any(|card| card["id"] == *id),
            "{id} stays hidden"
        );
    }
}

#[test]
fn pr_438_takes_a_revealed_cute_card_and_rushes_beside_two_more_cute_followers() {
    let mut initial = in_hand("PR-438", &[("found", "CSD02a-004"), ("below", "BP01-171")]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"cute1","card":"CSD02a-004"},{"id":"cute2","card":"CSD02a-004"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["deck"], json!(["below"]));
    assert_revealed(&game, &["found"], &["below"]);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進")),
        "three cute followers grant rush"
    );
}

#[test]
fn csd02a_003_leaves_a_non_cute_top_card_and_has_no_rush_alone() {
    let mut game = game(&in_hand(
        "CSD02a-003",
        &[("top", "BP01-171"), ("below", "CSD02a-004")],
    ));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["deck"], json!(["top", "below"]));
    assert_revealed(&game, &[], &["top", "below"]);
    assert!(
        !state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
}

#[test]
fn ecp01_009_takes_an_uma_musume_card_then_sacrifices_itself_for_two_life() {
    let mut game = game(&in_hand(
        "ECP01-009",
        &[("found", "ECP01-009"), ("below", "BP01-171")],
    ));
    play(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["hand"], json!(["found"]));
    assert_revealed(&game, &["found"], &["below"]);
    activate(&mut game, "ECP01-009");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn ecp02_sp11_spends_two_lessons_once_per_turn_to_take_an_idol_card() {
    let mut initial = on_field(
        "ECP02-SP11",
        &[("found", "CSD02a-004"), ("below", "BP01-171")],
    );
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"item1","card":"CP02-T01"},{"id":"item2","card":"CP02-T01"},
        {"id":"item3","card":"CP02-T01"},{"id":"item4","card":"CP02-T01"}
    ]);
    let mut game = game(&initial);
    activate(&mut game, "ECP02-SP11");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
    assert_revealed(&game, &["found"], &["below"]);
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", printed_activation("ECP02-SP11"), actor())
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "the second activation this turn is refused"
    );
}

#[test]
fn csd02a_006_evolution_takes_one_cute_card_and_bottoms_the_rest() {
    let mut initial = on_field(
        "CSD02a-005",
        &[
            ("other", "BP01-171"),
            ("found", "CSD02a-004"),
            ("spare", "CSD02a-004"),
            ("unseen", "BP01-037"),
        ],
    );
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CSD02a-006"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["deck"], json!(["unseen", "other", "spare"]));
    assert_revealed(&game, &["found"], &["other", "spare", "unseen"]);
}

#[test]
fn csd03a_009_finds_a_royal_paladin_among_five() {
    let mut game = game(&in_hand(
        "CSD03a-009",
        &[
            ("a", "BP01-171"),
            ("b", "BP01-037"),
            ("c", "BP01-171"),
            ("found", "CSD03a-009"),
            ("e", "BP01-171"),
            ("unseen", "CSD03a-009"),
        ],
    ));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["deck"], json!(["unseen", "a", "b", "c", "e"]));
    assert_revealed(&game, &["found"], &["a", "b", "c", "e", "unseen"]);
}

#[test]
fn dsd01b_011_evolution_takes_only_a_martial_dragonewt_follower() {
    let mut initial = on_field(
        "DSD01b-010",
        &[
            ("goblin", "BP01-171"),
            ("found", "DSD01b-010"),
            ("sage", "BP01-037"),
            ("last", "BP01-171"),
        ],
    );
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"DSD01b-011"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["deck"], json!(["goblin", "sage", "last"]));
    assert_revealed(&game, &["found"], &["goblin", "sage", "last"]);
}

#[test]
fn cp04_113_takes_a_revealed_princess_connect_card() {
    let mut game = game(&in_hand(
        "CP04-113",
        &[("found", "CP04-113"), ("below", "BP01-171")],
    ));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["deck"], json!(["below"]));
    assert_revealed(&game, &["found"], &["below"]);
}

#[test]
fn cp02_001_takes_a_passion_card_and_bottoms_the_other_two() {
    let mut game = game(&in_hand(
        "CP02-001",
        &[
            ("goblin", "BP01-171"),
            ("found", "PR-439"),
            ("sage", "BP01-037"),
            ("unseen", "PR-439"),
        ],
    ));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["deck"], json!(["unseen", "goblin", "sage"]));
    assert_revealed(&game, &["found"], &["goblin", "sage", "unseen"]);
}
