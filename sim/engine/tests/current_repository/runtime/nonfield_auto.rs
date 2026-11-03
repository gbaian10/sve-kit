#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
//! Subscriptions outside a field card's own events: deck-to-cemetery moves and cemetery observers.
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "nonfield-auto",
    )
    .unwrap()
}

fn decks(initial: &mut Value) {
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = Value::Array(
            (0..6_u8)
                .map(|i| json!({"id":format!("{seat}-d{i}"),"card":"BP01-171"}))
                .collect(),
        );
    }
}

fn finish(game: &mut Game, mut step: CardPlay, pay: bool) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(pay),
            InputDomain::Targets(domain) => {
                Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

/// Resolves every waiting ability and returns the public labels in order.
fn triggers(game: &mut Game, pay: bool) -> Vec<String> {
    let mut labels = Vec::new();
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
        labels.push(options[0].event.clone());
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step, pay);
    }
    labels
}

fn play(game: &mut Game, id: &str, seat: u32) -> Vec<String> {
    let step = game
        .play_internal_card(id, PlayerId { seat })
        .unwrap();
    finish(game, step, true);
    triggers(game, true)
}

#[test]
fn bp05_029_gains_attack_for_each_opponent_card_milled_on_your_turn() {
    let mut initial = draw_position();
    decks(&mut initial);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP05-029"},{"id":"thief","card":"BP10-031"}
    ]);
    let mut game = game(&initial);
    let labels = play(&mut game, "source", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["P2-d0"]), "its fanfare mills one");
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
    assert!(labels.iter().any(|label| label.starts_with("Move deck to cemetery of player 2")));
    play(&mut game, "thief", 1);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["cemetery"].as_array().unwrap().len(), 3);
    assert_eq!(after["objects"]["source"]["power"], 4_u8, "two milled cards, two triggers");
}

#[test]
fn bp05_029_ignores_its_own_deck_and_the_opponent_turn() {
    for (active, mill) in [("P1", "P1"), ("P2", "P2")] {
        let mut initial = draw_position();
        decks(&mut initial);
        initial["turn"]["active"] = json!(active);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-029"}]);
        initial["players"][mill]["zones"]["hand"] = json!([{"id":"miller","card":"CP03-093"}]);
        let mut game = game(&initial);
        play(&mut game, "miller", if mill == "P1" { 1 } else { 2 });
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[mill]["cemetery"].as_array().unwrap().len(), 1);
        assert_eq!(state["objects"]["source"]["power"], 1_u8, "{mill} milled on {active}'s turn");
    }
}

#[test]
fn bp07_041_pays_one_to_move_from_cemetery_to_ex_when_tetra_enters() {
    for pay in [true, false] {
        let mut initial = draw_position();
        decks(&mut initial);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"sonic","card":"BP07-041"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"tetra","card":"BP07-035"}]);
        let mut game = game(&initial);
        let step = game.play_internal_card("tetra", PlayerId { seat: 1 }).unwrap();
        finish(&mut game, step, pay);
        triggers(&mut game, pay);
        let state = game.projection(View::Referee).unwrap();
        let ex = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.contains(&json!("sonic")), pay);
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("sonic")), !pay);
        assert_eq!(state["P1"]["pp"]["current"], if pay { 6_u8 } else { 7_u8 });
    }
}

#[test]
fn bp07_041_ignores_an_opponent_tetra_and_other_followers() {
    for (seat, card) in [("P2", "BP07-035"), ("P1", "BP01-171")] {
        let mut initial = draw_position();
        decks(&mut initial);
        initial["turn"]["active"] = json!(seat);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"sonic","card":"BP07-041"}]);
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"tetra","card":card}]);
        let mut game = game(&initial);
        play(&mut game, "tetra", if seat == "P1" { 1 } else { 2 });
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["sonic"]), "{seat} {card}");
    }
}

#[test]
fn bp07_041_spell_deals_two_to_an_enemy_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"sonic","card":"BP07-041"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-126"}]);
    let mut game = game(&initial);
    play(&mut game, "sonic", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["sonic"]));
}

#[test]
fn cemetery_entry_observers_reject_their_own_entry_and_other_bodies() {
    let source = inputs().2["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "BP07-041")
        .unwrap();
    let mut candidate = source["input"].clone();
    lowerer().lower_current(&candidate).unwrap();
    let face = candidate["tree"]["cards"]["BP07-041"]["faces"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    let auto = face["abilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|ability| ability.get("auto").is_some())
        .unwrap()
        .clone();
    // A card in the cemetery never observes its own entry to the field.
    let mut itself = auto;
    itself["auto"]["on"]["enter_field"] = json!({"ref":"this"});
    let mut other = candidate.clone();
    set_auto(&mut other, itself);
    assert_eq!(lowerer().lower_current(&other).unwrap_err().code, "unsupported_construct");
}

fn set_auto(candidate: &mut Value, auto: Value) {
    let face = candidate["tree"]["cards"]["BP07-041"]["faces"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    let slot = face["abilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|ability| ability.get("auto").is_some())
        .unwrap();
    *slot = auto;
}

#[test]
fn bp07_058_in_ex_deals_one_per_five_nature_cards_at_your_end_phase() {
    for (nature, life) in [(4_usize, 20_u8), (10, 18)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"shadow","card":"BP07-058"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
            (0..nature)
                .map(|i| json!({"id":format!("n{i}"),"card":"BP07-058"}))
                .collect(),
        );
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
        triggers(&mut game, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["leader"]["life"], life, "{nature} nature cards");
    }
}

#[test]
fn bp07_058_spell_stays_in_ex_only_with_vaidi_and_cannot_be_played_from_ex() {
    for vaidi in [true, false] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"shadow","card":"BP07-058"}]);
        if vaidi {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"vaidi","card":"BP07-052"}]);
        }
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-126"}]);
        let mut game = game(&initial);
        play(&mut game, "shadow", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state["P1"]["ex"].as_array().unwrap().contains(&json!("shadow")), vaidi);
        if vaidi {
            game.play_internal_card("shadow", PlayerId { seat: 1 })
                .unwrap_err();
        }
    }
}
