use super::ability_combinations::{finish, loaded, play, position, view};
use super::*;

#[test]
fn pyutone_banishes_chosen_cards_directly_from_the_deck_on_entry() {
    let mut initial = position("BP04-059", "hand");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"removed","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["banish"], json!(["removed"]));
    assert_eq!(state["P1"]["deck"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
}

#[test]
fn evolved_pyutone_leaves_unselected_cards_in_the_deck() {
    let mut initial =
        ability_combinations::evolve_position("BP04-SL11", "BP04-SL12");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-173"}
    ]);
    let mut game = loaded(&initial);
    game.internal_rule_action(&json!({
        "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}
    }))
    .unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("evolve trigger");
    };
    let CardPlay::Awaiting(search) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
        .unwrap()
    else {
        panic!("deck search");
    };
    let InputDomain::Search(domain) = search.domain else {
        panic!("deck search cards");
    };
    let second = domain.cards.iter().find(|card| card.card == "BP01-173").unwrap();
    let step = game
        .answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![second.handle])),
        )
        .unwrap();
    finish(&mut game, Some(step), &[]);
    assert_eq!(view(&game)["P1"]["banish"], json!(["second"]));
    assert_eq!(view(&game)["P1"]["deck"], json!(["first"]));
}

#[test]
fn reversal_clears_both_ex_zones_before_placing_cards_from_the_deck() {
    let mut initial = position("BP10-008", "hand");
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"own-ex","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["ex"] =
        json!([{"id":"enemy-ex","card":"BP01-173"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"new-ex","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["ex"], json!(["new-ex"]));
    assert_eq!(state["P1"]["cemetery"], json!(["own-ex", "source"]));
    assert_eq!(state["P2"]["ex"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy", "enemy-ex"]));
}

#[test]
fn sword_almiraj_sets_current_life_to_the_printed_value() {
    let mut initial = position("BP19-107", "field");
    initial["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"hp":2_u8,"max_hp":9_u8});
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish(&mut game, None, &[]);
    assert_eq!(view(&game)["objects"]["source"]["hp"], 6_u8);
}

#[test]
fn dragonification_tower_creates_the_token_for_the_destroyed_targets_controller() {
    let mut game = loaded(&position("BP01-091", "field"));
    let step = union_burst_batch1::activate(&mut game, "BP01-091");
    finish(&mut game, Some(step), &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    let field = state["P2"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 1);
    let token = field[0].as_str().unwrap();
    assert_eq!(state["objects"][token]["card"], "BP01-T11");
}

#[test]
fn primordial_dragonkeeper_observes_the_cost_at_entry() {
    let mut initial = position("BP01-078", "field");
    initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap()
        .push(json!({"id":"observer","card":"BP04-U04"}));
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"discounted","card":"BP01-T11","state":{"cost":3_u8}}
    ]);
    let mut game = loaded(&initial);
    ability_combinations::activate_card(&mut game, "BP01-078", 2, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["field"], json!(["source", "observer", "discounted"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn rin_has_no_formal_target_if_uzuki_is_absent() {
    let mut initial = position("CP02-021", "hand");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"mio","card":"CP02-022"}]);
    let mut game = loaded(&initial);
    let step = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
    let CardPlay::Awaiting(entry) = step else {
        panic!("Ward entry decision");
    };
    game.answer_internal_card(entry.actor, entry.token, Answer::WardEntry(false))
        .unwrap();
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("pending Fanfare");
    };
    let first = options.iter().find(|option| option.line == 2).unwrap();
    let fanfare = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(first.handle))
        .unwrap();
    assert!(!matches!(&fanfare, CardPlay::Awaiting(question) if matches!(question.domain, InputDomain::Targets(_))));
    finish(&mut game, Some(fanfare), &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
    assert_eq!(state["P2"]["leader"]["life"], 15_u8);
}
