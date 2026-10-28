#![expect(
    clippy::panic,
    reason = "An unexpected authority question invalidates these printed-card scenarios."
)]
use super::*;

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: seat_number(seat),
    }
}

fn enter(game: &mut Game, seat: &str) {
    let CardPlay::Entered(entry) = game.play_internal_card("source", actor(seat)).unwrap() else {
        panic!("the real base enters from hand");
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn evolve(game: &mut Game) {
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the real base pays its printed evolution cost"
    );
    assert!(
        step.events.iter().any(|event| event["kind"] == "進化"
            && event["object"] == "source"
            && event["with"] == "form"),
        "a real evolution event links the two printed cards"
    );
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger");
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn machine_position(seat: &str, target: bool) -> Value {
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["field"] = json!([]);
    initial["players"][seat]["pp"]["current"] = json!(4_u8);
    initial["players"][seat]["zones"]["hand"] = json!([{"id":"source","card":"BP17-049"}]);
    initial["players"][seat]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-050"}]);
    initial["players"][opponent]["zones"]["field"] = if target {
        json!([{"id":"enemy","card":"BP01-037"}])
    } else {
        json!([])
    };
    initial
}

fn machine_evolution(game: &mut Game, seat: &str, target: bool) {
    enter(game, seat);
    evolve(game);
    let play = trigger(game, "BP17-050");
    if target {
        let CardPlay::Awaiting(question) = play else {
            panic!("formal evolve target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("target domain");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "only the opposing follower is a legal target"
        );
        let before = game.projection(View::Referee).unwrap();
        let wrong = PlayerId {
            seat: if seat == "P1" { 2 } else { 1 },
        };
        game.answer_internal_card(
            wrong,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "a wrong actor cannot change the pending target or state"
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8,
            "the evolved card deals its printed two damage"
        );
    } else {
        assert_eq!(
            play,
            CardPlay::Cancelled,
            "without an opposing follower the mandatory trigger is unplayable"
        );
    }
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["source"]["evolved_with"], "form",
        "the source remains linked to its real evolved card"
    );
    assert_eq!(
        state[seat]["evolution"],
        json!(["form"]),
        "the real evolved card is linked to its base"
    );
    assert_eq!(
        state[seat]["pp"]["current"], 1_u8,
        "base play and ordinary evolution spend exactly three PP"
    );
}

fn destroy_machine(game: &mut Game, seat: &str) {
    activate(game, seat);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state[seat]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the departed base enters its owner cemetery"
    );
    assert_eq!(
        state[seat]["evolution"],
        json!([]),
        "departure releases the ordinary evolution link"
    );
    assert_eq!(
        state[seat]["evolve_deck"],
        json!(["form"]),
        "the form returns to its owner evolve deck"
    );
    assert_eq!(
        state["objects"]["form"]["face_up"], true,
        "the released form returns face up"
    );
    assert!(
        state["objects"]["source"]["evolved_with"].is_null(),
        "the departed base keeps no stale form reference"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Lastword");
    };
    assert_eq!(
        options.len(),
        1,
        "only the evolved printed Lastword becomes pending"
    );
    assert_eq!(
        options[0].card, "BP17-050",
        "the Lastword originates from the real form"
    );
    assert_eq!(
        options[0].line, 2,
        "the Lastword retains its printed source line"
    );
    resolved(trigger(game, "BP17-050"));
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the single departure trigger finishes without duplication"
    );
}

#[test]
fn bp17_050_real_evolution_and_departure_create_a_playable_repair_mode_for_both_seats() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut game = with_producer(
            &machine_position(seat, true),
            Action::Destroy {
                objects: subjects(),
            },
        );
        machine_evolution(&mut game, seat, true);
        destroy_machine(&mut game, seat);
        let before = game.projection(View::Referee).unwrap();
        let ex = before[seat]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 1);
        let repair = ex[0].as_str().unwrap();
        assert_eq!(before["objects"][repair]["card"], "BP07-T02");
        resolved(game.play_internal_card(repair, actor(seat)).unwrap());
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after[seat]["leader"]["life"], 21_u8);
        assert_eq!(after[opponent]["leader"]["life"], 20_u8);
        assert_eq!(after[seat]["pp"]["current"], 0_u8);
        assert_eq!(after[seat]["ex"], json!([]));
        assert!(after["objects"][repair].is_null());
    }
}

#[test]
fn bp17_050_without_an_evolve_target_keeps_its_lastword_but_cannot_overfill_ex() {
    let mut initial = machine_position("P1", false);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"a","card":"BP01-037"},{"id":"b","card":"BP01-037"},
        {"id":"c","card":"BP01-037"},{"id":"d","card":"BP01-037"},{"id":"e","card":"BP01-037"}
    ]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    machine_evolution(&mut game, "P1", false);
    destroy_machine(&mut game, "P1");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["a", "b", "c", "d", "e"]));
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
}

#[test]
fn bp17_050_moved_to_ex_releases_the_form_without_firing_lastword() {
    let mut game = with_producer(&machine_position("P1", false), movement(OwnerZone::Ex));
    machine_evolution(&mut game, "P1", false);
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["ex"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(state["P1"]["evolve_deck"], json!(["form"]));
    assert_eq!(state["P1"]["evolution"], json!([]));
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the single departure trigger finishes without duplication"
    );
    assert!(
        state["objects"]
            .as_object()
            .unwrap()
            .values()
            .all(|object| object["card"] != "BP07-T02")
    );
}

fn witch_position(deck_size: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP08-038"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-039"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..deck_size)
            .map(|n| json!({"id":format!("draw-{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ally","card":"BP01-037"}, {"id":"amulet","card":"BP18-115"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"keep-ex","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"opposing-base","card":"BP17-049","state":{"evolved":true,"evolved_with":"opposing-form"}},
        {"id":"enemy","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["evolution"] =
        json!([{"id":"opposing-form","card":"BP17-050"}]);
    initial
}

#[test]
fn bp08_038_draws_deck_minus_five_then_039_puts_all_other_field_cards_in_cemetery() {
    for size in [3_u8, 5, 8] {
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &witch_position(size),
            &Value::Null,
            &Value::Null,
            "unlock-witch",
        )
        .unwrap();
        enter(&mut game, "P1");
        resolved(trigger(&mut game, "BP08-038"));
        game.resume_internal_card().unwrap();
        let drawn = size.saturating_sub(5);
        let after_draw = game.projection(View::Referee).unwrap();
        assert_eq!(
            after_draw["P1"]["hand"].as_array().unwrap().len(),
            usize::from(drawn)
        );
        assert_eq!(
            after_draw["P1"]["deck"].as_array().unwrap().len(),
            usize::from(size - drawn)
        );
        evolve(&mut game);
        resolved(trigger(&mut game, "BP08-039"));
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!(["ally", "amulet"]));
        assert_eq!(state["P2"]["cemetery"], json!(["opposing-base", "enemy"]));
        assert_eq!(state["P1"]["evolution"], json!(["form"]));
        assert_eq!(state["P2"]["evolution"], json!([]));
        assert_eq!(state["P2"]["evolve_deck"], json!(["opposing-form"]));
        assert_eq!(state["objects"]["opposing-form"]["face_up"], true);
        assert_eq!(
            state["objects"]["source"]["evolved_with"], "form",
            "the source remains linked to its real evolved card"
        );
        assert_eq!(state["P1"]["ex"], json!(["keep-ex"]));
        resolved(trigger(&mut game, "BP18-115"));
        game.resume_internal_card().unwrap();
        resolved(trigger(&mut game, "BP17-050"));
        game.resume_internal_card().unwrap();
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 21_u8);
        assert_eq!(after["P1"]["ex"], json!(["keep-ex"]));
        assert_eq!(
            after["P2"]["ex"].as_array().unwrap().len(),
            1,
            "the opposing evolved 050 Lastword creates one Repair Mode"
        );
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the single departure trigger finishes without duplication"
        );
    }
}
