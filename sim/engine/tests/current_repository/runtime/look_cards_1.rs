#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Look scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn game(state: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "look-cards",
    )
    .unwrap()
}

fn position(source: &str, deck: &[(&str, &str)]) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":source},{"id":"spare","card":"BP01-171"}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        deck.iter()
            .map(|(id, card)| json!({"id":id,"card":card}))
            .collect::<Vec<_>>()
    );
    state
}

/// Answers every question; observed choices take the printed numbers in `picked`.
fn settle(game: &mut Game, mut play: CardPlay, picked: &[&str], offered: &mut Vec<String>) {
    loop {
        match play {
            CardPlay::Awaiting(q) => {
                let answer = answer(&q, picked, offered);
                play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
            }
            // Cancelled also starts a scenario that begins with a pending trigger.
            CardPlay::Entered(_) | CardPlay::Resolved(_) | CardPlay::Cancelled => {
                while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                    game.resume_internal_card().unwrap();
                }
                let Some(q) = game.internal_pending_choice().unwrap() else {
                    return;
                };
                let InputDomain::Pending(options) = &q.domain else {
                    panic!("pending trigger")
                };
                play = game
                    .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                    .unwrap();
            }
            CardPlay::Continued(_) => panic!("unexpected play outcome"),
        }
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only the printed Look, cost, entry and order questions belong to these cards."
)]
fn answer(q: &DecisionView, picked: &[&str], offered: &mut Vec<String>) -> Answer {
    match &q.domain {
        // A Look choice offers printed numbers in `picked`; other targets pay their minimum.
        InputDomain::Targets(domain)
            if domain
                .members
                .iter()
                .any(|m| m.card.as_deref().is_some_and(|c| picked.contains(&c))) =>
        {
            offered.extend(domain.members.iter().filter_map(|m| m.card.clone()));
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .filter(|m| m.card.as_deref().is_some_and(|c| picked.contains(&c)))
                    .map(|m| m.handle)
                    .collect(),
            )
        }
        InputDomain::Targets(domain) => Answer::Targets(
            domain
                .members
                .iter()
                .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::OptionalCost => Answer::OptionalCost(true),
        InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        InputDomain::CostMaterials(domain) => Answer::CostMaterials(
            domain
                .members
                .iter()
                .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                .map(|m| m.handle)
                .collect(),
        ),
        _ => panic!("unexpected question"),
    }
}

fn union_to_hand(source: &str, deck: &[(&str, &str)], picked: &str, offered: &[&str]) -> Value {
    let mut game = game(&position(source, deck));
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let mut seen = Vec::new();
    settle(&mut game, play, &[picked], &mut seen);
    assert_eq!(seen, offered, "{source} offers exactly the printed union");
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp12_107_takes_wilds_or_nature_and_heals_with_wilds_on_field_or_ex() {
    for (zone, life) in [("field", 11_u8), ("ex", 11), ("cemetery", 10)] {
        let deck = [("wilds", "BP11-001"), ("nature", "BP07-001"), ("goblin", "BP01-171"), ("tail", "BP01-171")];
        let mut state = position("BP12-107", &deck);
        state["players"]["P1"]["leader"]["life"] = json!(10_u8);
        state["players"]["P1"]["zones"][zone] = json!([{"id":"camp","card":"BP11-008"}]);
        let mut game = game(&state);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let mut seen = Vec::new();
        settle(&mut game, play, &["BP07-001"], &mut seen);
        assert_eq!(seen, ["BP11-001", "BP07-001"]);
        let view = game.projection(View::Referee).unwrap();
        assert!(view["P1"]["hand"].as_array().unwrap().contains(&json!("nature")));
        assert_eq!(view["P1"]["leader"]["life"], life, "only a wilds card on the field or in EX heals 1");
    }
}

#[test]
fn cp01_069_takes_an_uma_musume_card_or_any_amulet() {
    let view = union_to_hand(
        "CP01-069",
        &[("goblin", "BP01-171"), ("amulet", "BP01-016"), ("uma", "CP01-001"), ("tail", "BP01-171")],
        "BP01-016",
        &["BP01-016", "CP01-001"],
    );
    assert!(view["P1"]["hand"].as_array().unwrap().contains(&json!("amulet")));
    assert_eq!(view["P1"]["deck"][0], json!("tail"));
}

#[test]
fn cp02_028_takes_a_cool_or_passion_card() {
    let view = union_to_hand(
        "CP02-028",
        &[("cool", "CP02-006"), ("passion", "CP02-009"), ("tail", "BP01-171")],
        "CP02-009",
        &["CP02-006", "CP02-009"],
    );
    assert!(view["P1"]["hand"].as_array().unwrap().contains(&json!("passion")));
    assert_eq!(view["P1"]["deck"], json!(["tail", "cool"]));
}

#[test]
fn cp04_047_union_burst_fanfare_takes_a_club_card_or_priconne_spell() {
    let mut state = position(
        "CP04-047",
        &[("goblin", "BP01-171"), ("spell", "CP04-044"), ("club", "CP04-047"), ("other", "BP01-016"), ("tail", "BP01-171")],
    );
    state["players"]["P1"]["construction"] = json!("title");
    state["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    let mut game = game(&state);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let mut seen = Vec::new();
    settle(&mut game, play, &["CP04-044"], &mut seen);
    assert_eq!(seen, ["CP04-044", "CP04-047"]);
    let view = game.projection(View::Referee).unwrap();
    assert!(view["P1"]["hand"].as_array().unwrap().contains(&json!("spell")));
}

#[test]
fn bp13_101_discards_then_takes_a_spell_or_amulet() {
    let view = union_to_hand(
        "BP13-101",
        &[("goblin", "BP01-171"), ("spell", "BP01-007"), ("amulet", "BP01-016"), ("other", "BP01-034"), ("tail", "BP01-171")],
        "BP01-016",
        &["BP01-007", "BP01-016"],
    );
    assert_eq!(view["P1"]["cemetery"], json!(["spare"]), "the hand card pays the cost");
    assert!(view["P1"]["hand"].as_array().unwrap().contains(&json!("amulet")));
}

#[test]
fn cp03_111_and_cp03_106_order_chosen_cards_on_top_and_the_rest_on_the_bottom() {
    for (source, deck) in [
        ("CP03-111", json!(["one", "two", "three", "four", "five", "tail"])),
        ("CP03-106", json!(["one", "two", "tail"])),
    ] {
        let ids: Vec<&str> = deck.as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
        let cards: Vec<(&str, &str)> = ids
            .iter()
            .map(|id| (*id, if *id == "two" { "BP01-034" } else { "BP01-171" }))
            .collect();
        let mut game = game(&position(source, &cards));
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let mut seen = Vec::new();
        settle(&mut game, play, &["BP01-034"], &mut seen);
        let view = game.projection(View::Referee).unwrap();
        let after = view["P1"]["deck"].as_array().unwrap();
        assert_eq!(after[0], json!("two"), "{source} keeps the chosen card on top");
        assert_eq!(after.len(), ids.len());
        assert!(
            after.iter().position(|id| id == "tail") < after.iter().position(|id| id == "one"),
            "{source} sends the unchosen cards below the unseen tail"
        );
    }
}

#[test]
fn bp13_041_needs_six_original_costs_for_its_damage() {
    // Q1746: without an enemy follower the ability is not played, so nothing is drawn.
    for (costs, enemy, destroyed, hand) in [(6_usize, true, true, 1_usize), (5, true, false, 1), (6, false, false, 0)] {
        let mut state = draw_position();
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP13-040"}]);
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-041"}]);
        state["players"]["P1"]["zones"]["deck"] = json!([{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"}]);
        state["players"]["P1"]["zones"]["cemetery"] = json!(
            ["BP01-012", "BP01-002", "BP01-009", "BP01-021", "BP01-006", "BP01-104"][..costs]
                .iter()
                .enumerate()
                .map(|(i, card)| json!({"id":format!("grave{i}"),"card":card}))
                .collect::<Vec<_>>()
        );
        if enemy {
            state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-034"}]);
        }
        let mut game = game(&state);
        let play = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":2_u8}}),
            )
            .unwrap();
        assert_eq!(play.outcome, "resolved");
        settle(&mut game, CardPlay::Cancelled, &[], &mut Vec::new());
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            enemy && view["P2"]["field"] == json!([]),
            destroyed,
            "4 damage needs six original cost kinds"
        );
        assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), hand, "draw 2, discard 1");
    }
}

#[test]
fn bp13_101_declined_discard_looks_at_nothing() {
    let deck = [("goblin", "BP01-171"), ("spell", "BP01-007"), ("tail", "BP01-171")];
    let mut game = game(&position("BP13-101", &deck));
    let entered = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(entered, CardPlay::Entered(_)), "BP13-101 enters first");
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let mut play = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
        .unwrap();
    if let CardPlay::Awaiting(cost) = &play
        && matches!(cost.domain, InputDomain::OptionalCost)
    {
        play = game
            .answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(false))
            .unwrap();
    }
    settle(&mut game, play, &[], &mut Vec::new());
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["spare"]), "the cost is not paid");
    assert_eq!(view["P1"]["deck"], json!(["goblin", "spell", "tail"]), "nothing is looked at or moved");
}
