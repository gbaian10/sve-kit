#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed reveal scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "reveal-deck-top",
    )
    .unwrap()
}

fn settle(game: &mut Game, mut step: Option<CardPlay>) {
    loop {
        while let Some(CardPlay::Awaiting(q)) = step {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .take(
                            d.cardinality
                                .maximum
                                .map_or(d.members.len(), |n| usize::try_from(n.0).unwrap()),
                        )
                        .map(|m| m.handle)
                        .collect(),
                ),
                InputDomain::Order(cards) => {
                    Answer::Order(cards.iter().map(|c| c.handle).collect())
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::Integer(_) => Answer::Integer(Nat(1)),
                InputDomain::SearchPartition(d) => Answer::SearchPartition(
                    d.cards
                        .iter()
                        .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                        .map(|c| c.handle)
                        .collect(),
                ),
                _ => panic!("unexpected reveal decision"),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending")
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
}

fn activate(game: &mut Game, card: &str) {
    let id = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, PLAYER).unwrap();
    settle(game, Some(step));
}

#[test]
fn cp03_115_all_printings_reveal_the_fixed_top_card_and_return_the_matching_half_moon() {
    for card in ["CP03-115", "CP03-P79"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"top","card":"CP03-105"},{"id":"tail","card":"CP03-052"}]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", PLAYER).unwrap();
        settle(&mut game, Some(step));
        activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["deck"], json!(["tail"]));
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("top"))
        );
    }
}

#[test]
fn cp03_105_all_printings_damage_with_the_revealed_original_cost_without_moving_a_nonmatch() {
    for card in ["CP03-105", "CP03-SL21"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"top","card":"CP03-052"},{"id":"tail","card":"CP03-058"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
        let mut game = game(&initial);
        activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["deck"], json!(["top", "tail"]));
        assert_eq!(state["objects"]["enemy"]["hp"], 19_u8);
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        let opponent = game.projection(View::P2).unwrap();
        assert!(
            opponent["objects"]["top"].is_null(),
            "the revelation expires after resolution"
        );
    }
}

#[test]
fn bp08_074_all_printings_follow_only_a_revealed_cost_two_card_into_ex() {
    for card in ["BP08-074", "PR-207", "SP01-037", "SP01-SL37"] {
        for top in [Some("CP03-103"), Some("CP03-058"), None] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            initial["players"]["P1"]["zones"]["deck"] = json!(
                top.into_iter()
                    .map(|top_card| json!({"id":"top","card":top_card}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            activate(&mut game, card);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["ex"],
                if top == Some("CP03-103") {
                    json!(["top"])
                } else {
                    json!([])
                }
            );
            if top == Some("CP03-103") {
                let step = game.play_internal_card("top", PLAYER).unwrap();
                settle(&mut game, Some(step));
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    9_u8,
                    "the moved generation receives the discount"
                );
            }
        }
    }
}

fn play_revealer(card: &str, top: Option<&str>) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        top.into_iter()
            .map(|top_card| json!({"id":"top","card":top_card}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    let step = game.play_internal_card("source", PLAYER).unwrap();
    settle(&mut game, Some(step));
    game
}

#[test]
fn bp02_117_buffs_itself_by_the_revealed_cost_and_keeps_the_card_in_place() {
    let game = play_revealer("BP02-117", Some("CP03-103"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
    assert_eq!(state["P1"]["deck"], json!(["top"]));
}

#[test]
fn bp19_049_buffs_the_formal_target_then_draws_the_same_revealed_card() {
    let game = play_revealer("BP19-049", Some("CP03-103"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["hp"], 6_u8);
    assert_eq!(state["P1"]["hand"], json!(["top"]));
}

#[test]
fn bp21_066_all_printings_distinguish_dragon_entry_other_draw_and_empty_deck() {
    for card in ["BP21-066", "BP21-P35"] {
        for top in [Some("BP01-081"), Some("CP03-103"), None] {
            let game = play_revealer(card, top);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"],
                if top == Some("CP03-103") {
                    json!(["top"])
                } else {
                    json!([])
                }
            );
            if top == Some("BP01-081") {
                assert!(
                    state["P1"]["field"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("top"))
                );
            }
            assert_eq!(state["P1"]["deck"], json!([]));
        }
    }
}

#[test]
fn cp01_068_all_printings_damage_and_heal_by_the_same_public_cost() {
    for card in ["CP01-068", "CP01-SL18", "CP01-SP18"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"CP03-103"}]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", PLAYER).unwrap();
        settle(&mut game, Some(step));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P2"]["leader"]["life"], 18_u8);
        assert_eq!(state["P1"]["deck"], json!(["top"]));
        goldship_activated_buff(card);
    }
}

fn goldship_activated_buff(card: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    let mut game = game(&initial);
    let ability = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == 3)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PLAYER)
        .unwrap();
    settle(&mut game, Some(step));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["source"]["power"], 18_u8,
        "the printed line-three activation adds ten attack"
    );
    assert_eq!(
        state["objects"]["source"]["hp"], 18_u8,
        "the printed line-three activation adds ten life"
    );
}

#[test]
fn cp01_083_all_printings_distinguish_odd_even_and_no_revealed_card() {
    for card in ["CP01-083", "CP01-P63"] {
        for top in [Some("CP03-052"), Some("CP03-103"), None] {
            let game = play_revealer(card, top);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if top == Some("CP03-103") {
                    18_u8
                } else {
                    20_u8
                }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if top == Some("CP03-052") {
                    18_u8
                } else {
                    20_u8
                }
            );
        }
    }
}

#[test]
fn csd02c_006_all_printings_damage_on_real_evolution_using_original_cost() {
    for card in ["CSD02c-006", "CSD02c-P03"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CSD02c-005"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"CP03-103"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
        let mut game = game(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        settle(&mut game, None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 18_u8);
        assert_eq!(state["P1"]["deck"], json!(["top"]));
    }
}

#[test]
fn pcs01_048_enters_only_a_revealed_princess_follower_and_grants_it_ward() {
    for top in [Some("CP04-015"), Some("CP03-052"), None] {
        let game = play_revealer("PCS01-048", top);
        let state = game.projection(View::Referee).unwrap();
        if top == Some("CP04-015") {
            assert!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("top"))
            );
            assert!(
                state["objects"]["top"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護"))
            );
        } else {
            assert_eq!(
                state["P1"]["deck"],
                if top.is_some() {
                    json!(["top"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn cp03_105_all_printings_return_the_matching_full_moon_after_cost_damage() {
    for card in ["CP03-105", "CP03-SL21"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"CP03-104"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
        let mut game = game(&initial);
        activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 15_u8);
        assert_eq!(state["P1"]["hand"], json!(["top"]));
    }
}

#[test]
fn public_reveal_entry_keeps_the_observed_card_in_deck_when_the_field_is_full() {
    for (card, top) in [("BP21-066", "BP01-081"), ("PCS01-048", "CP04-015")] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0_u8..4)
                .map(|n| json!({"id":format!("ally{n}"),"card":"CP03-052"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":top}]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", PLAYER).unwrap();
        settle(&mut game, Some(step));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["deck"], json!(["top"]));
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    }
}
