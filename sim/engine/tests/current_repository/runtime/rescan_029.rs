#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these real card scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, field: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][if field { "field" } else { "hand" }] =
        json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"},
        {"id":"third","card":"BP01-037"}
    ]);
    initial
}

fn play(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("source", actor()).unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters the field"
    );
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("fanfare");
    };
    assert_eq!(options.len(), 1, "exactly one Fanfare is pending");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn evolve(card: &str, form: &str, cost: u8) {
    let mut initial = position(card, true);
    initial["players"]["P1"]["pp"]["current"] = json!(cost);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let less = cost.saturating_sub(1);
    let mut insufficient = initial.clone();
    insufficient["players"]["P1"]["pp"]["current"] = json!(less);
    let mut denied = game(&insufficient);
    if let Ok(step) = denied.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":less}}),
    ) {
        assert_ne!(step.outcome, "resolved", "the printed cost is required");
    }
    assert_eq!(
        denied.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        less,
        "rejected evolution preserves PP"
    );
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the exact printed cost permits evolution"
    );
    let event = step
        .events
        .iter()
        .find(|event| event["kind"] == "進化")
        .unwrap();
    assert_eq!(event["object"], "source", "the real base evolves");
    assert_eq!(
        event["with"], "form",
        "the matching evolution resource is used"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8,
        "the full printed evolution cost is paid"
    );
}

fn creates_token(card: &str, token_card: &str, size: u8, after_source: u8) {
    let mut game = game(&position(card, false));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1, "exactly one token is created");
    assert_eq!(
        state["P2"]["ex"],
        json!([]),
        "the opponent EX remains unchanged"
    );
    let token = ex[0].as_str().unwrap().to_owned();
    assert_eq!(
        state["objects"][&token]["card"], token_card,
        "the admitted token printing supplies the object"
    );
    assert_eq!(
        state["objects"][&token]["power"], size,
        "token power comes from its snapshot"
    );
    assert_eq!(
        state["objects"][&token]["hp"], size,
        "token health comes from its snapshot"
    );
    assert_eq!(
        state["P1"]["field"],
        json!(["source"]),
        "the created token stays in EX"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], after_source,
        "only playing the source consumes PP"
    );
}

#[test]
fn bp20_040_and_alias_evolve_for_one_pp() {
    for card in ["BP20-040", "BP20-SL12"] {
        evolve(card, "BP20-041", 1);
    }
}

#[test]
fn bp20_050_and_alias_evolve_and_assail_attacks_a_standing_follower() {
    for card in ["BP20-050", "BP20-P24"] {
        evolve(card, "BP20-051", 1);
        let mut initial = position(card, true);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
    }
    let mut initial = position("BP20-040", true);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    assert_eq!(
        game(&initial)
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
}

#[test]
fn bp20_074_and_alias_evolve_for_one_pp() {
    for card in ["BP20-074", "BP20-SL20"] {
        evolve(card, "BP20-075", 1);
    }
}

#[test]
fn bp20_076_and_its_two_aliases_evolve_for_one_pp() {
    for card in ["BP20-076", "BP20-SL22", "PR-561"] {
        evolve(card, "BP20-077", 1);
    }
}

#[test]
fn bp20_079_and_promo_evolve_for_one_pp() {
    for card in ["BP20-079", "PR-542"] {
        evolve(card, "BP20-080", 1);
    }
}

#[test]
fn bp20_083_mills_exactly_two_own_top_cards_and_evolves_for_five() {
    let mut game = game(&position("BP20-083", false));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["first", "second"]));
    assert_eq!(state["P1"]["deck"], json!(["third"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    evolve("BP20-083", "BP20-084", 5);
}

#[test]
fn bp20_016_and_alias_create_one_fairy_wisp_in_ex() {
    for card in ["BP20-016", "BP20-P10"] {
        creates_token(card, "BP01-T02", 1, 8);
    }
}

#[test]
fn bp20_035_and_alias_create_one_steel_knight_in_ex() {
    for card in ["BP20-035", "BP20-P20"] {
        creates_token(card, "BP17-T12", 2, 9);
    }
}
