use super::*;

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: seat_number(seat),
    }
}

fn play_entry(game: &mut Game, object: &str, card: &str) -> Option<Activation> {
    let played = game.play_internal_card(object, actor("P1")).unwrap();
    let played = match played {
        CardPlay::Awaiting(question)
            if matches!(question.domain, InputDomain::WardEntry { .. }) =>
        {
            game.answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
                .unwrap()
        }
        CardPlay::Awaiting(_) => return None,
        other @ (CardPlay::Resolved(_)
        | CardPlay::Entered(_)
        | CardPlay::Cancelled
        | CardPlay::Continued(_)) => other,
    };
    if !matches!(played, CardPlay::Entered(_)) {
        return None;
    }
    let question = game.internal_pending_choice().unwrap()?;
    let InputDomain::Pending(options) = &question.domain else {
        return None;
    };
    if options.len() != 1 || options[0].card != card {
        return None;
    }
    Some(resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    ))
}

fn play_targeted(game: &mut Game, object: &str, card: &str) -> Option<Activation> {
    if !matches!(
        game.play_internal_card(object, actor("P1")).unwrap(),
        CardPlay::Entered(_)
    ) {
        return None;
    }
    let pending = game.internal_pending_choice().unwrap()?;
    let InputDomain::Pending(options) = &pending.domain else {
        return None;
    };
    if options.len() != 1 || options[0].card != card {
        return None;
    }
    let resolution = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(question) = resolution else {
        return None;
    };
    let InputDomain::Targets(domain) = &question.domain else {
        return None;
    };
    if domain.members.len() != 1 {
        return None;
    }
    Some(resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    ))
}

fn evolve(game: &mut Game, base: &str, form: &str, form_card: &str, pp: u8) -> Option<Activation> {
    game.internal_rule_action(
        &json!({"do":"evolve","source":base,"evolve_card":form,"pay":{"pp":pp}}),
    )
    .unwrap();
    let question = game.internal_pending_choice().unwrap()?;
    let InputDomain::Pending(options) = &question.domain else {
        return None;
    };
    assert_eq!(
        options.len(),
        1,
        "the evolved card should expose exactly one trigger"
    );
    assert_eq!(
        options[0].card, form_card,
        "the trigger should belong to the evolved face"
    );
    Some(resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    ))
}

#[test]
fn bp02_071_fanfare_damages_only_its_controller() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"dealer","card":"BP02-071"}]);
    let mut game = game(&initial);
    play_entry(&mut game, "dealer", "BP02-071").unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 17_i32,
        "the controller's leader takes 3 damage"
    );
    assert_eq!(
        state["P2"]["leader"]["life"], 20_i32,
        "the opponent's leader is unaffected"
    );
    assert_eq!(state["objects"]["dealer"]["keywords"], json!(["守護"]));
}

#[test]
fn bp02_059_fanfare_damages_the_chosen_opponent_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"siegfried","card":"BP02-059"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"target","card":"BP01-037"}]);
    let mut game = game(&initial);
    play_targeted(&mut game, "siegfried", "BP02-059").unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["target"]["hp"],
        3_i32,
        "the chosen follower takes 2 damage"
    );
}

#[test]
fn bp02_096_fanfare_draws_and_grants_ward() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"angel","card":"BP02-096"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    play_entry(&mut game, "angel", "BP02-096").unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["objects"]["angel"]["keywords"], json!(["守護"]));
}

#[test]
fn bp02_013_fanfare_recovers_three_life() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"healer","card":"BP02-013"}]);
    let mut game = game(&initial);
    play_entry(&mut game, "healer", "BP02-013").unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        13_i32,
        "the fanfare restores 3 life"
    );
}

#[test]
fn bp02_097_evolve_trigger_recovers_two_life_and_keeps_ward() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP02-096"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-097"}]);
    let mut game = game(&initial);
    evolve(&mut game, "base", "form", "BP02-097", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 12_i32,
        "the evolve trigger restores 2 life"
    );
    assert_eq!(state["objects"]["base"]["keywords"], json!(["守護"]));
}

#[test]
fn bp02_100_fanfare_recovers_two_life() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"cleric","card":"BP02-100"}]);
    let mut game = game(&initial);
    play_entry(&mut game, "cleric", "BP02-100").unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_i32,
        "the fanfare restores 2 life"
    );
}

#[test]
fn bp02_113_attack_trigger_recovers_two_life() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"unico","card":"BP02-113"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"unico","target":"P2.leader"}))
        .unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    assert!(
        matches!(question.domain, InputDomain::Pending(_)),
        "the attack should trigger BP02-113's ability"
    );
    let InputDomain::Pending(options) = &question.domain else {
        return;
    };
    assert_eq!(
        options[0].card, "BP02-113",
        "the base-face attack ability should trigger"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_i32,
        "the attack trigger restores 2 life"
    );
}

#[test]
fn bp02_114_attack_trigger_recovers_two_life_on_its_evolved_face() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP02-113"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-114"}]);
    let mut game = game(&initial);
    evolve(&mut game, "base", "form", "BP02-114", 1);
    game.internal_rule_action(&json!({"do":"attack","attacker":"base","target":"P2.leader"}))
        .unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    assert!(
        matches!(question.domain, InputDomain::Pending(_)),
        "the attack should trigger BP02-114's ability"
    );
    let InputDomain::Pending(options) = &question.domain else {
        return;
    };
    assert_eq!(
        options[0].card, "BP02-114",
        "the evolved-face attack ability should trigger"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_i32,
        "the attack trigger restores 2 life"
    );
}

#[test]
fn bp02_094_fanfare_destroys_a_selected_opposing_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"court","card":"BP02-094"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"target","card":"BP01-037"}]);
    let mut game = game(&initial);
    play_targeted(&mut game, "court", "BP02-094").unwrap();
    assert!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("target")),
        "the selected follower should be destroyed"
    );
}

fn assert_evolve_cost(base_card: &str, form_card: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":base_card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form_card}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the evolution should resolve"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_i32,
        "the one-PP discount leaves 9 PP"
    );
}

#[test]
fn bp02_022_evolves_for_one_pp() {
    assert_evolve_cost("BP02-022", "BP02-023");
}
#[test]
fn bp02_025_evolves_for_one_pp() {
    assert_evolve_cost("BP02-025", "BP02-026");
}
#[test]
fn bp02_031_evolves_for_one_pp() {
    assert_evolve_cost("BP02-031", "BP02-032");
}
