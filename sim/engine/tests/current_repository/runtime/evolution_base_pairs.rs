#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these real evolution paths."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"}, {"id":"second","card":"BP01-037"},
        {"id":"third","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "evolution-base-pairs",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, target_seat: u32) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("printed public target");
            };
            let member = domain
                .members
                .iter()
                .find(|member| member.player.seat == target_seat)
                .or_else(|| domain.members.first())
                .unwrap();
            Answer::Targets(vec![member.handle])
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn enter(game: &mut Game) {
    let play = game.play_internal_card("source", actor()).unwrap();
    let CardPlay::Entered(entry) = finish(game, play, 2) else {
        panic!("the real base enters");
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn trigger(game: &mut Game, printing: &str, target_seat: u32) -> bool {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("real printed trigger");
    };
    assert_eq!(options.len(), 1, "one real source trigger is pending");
    assert_eq!(
        options[0].card, printing,
        "the selected real printing supplies the ability"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let play = finish(game, play, target_seat);
    let cancelled = matches!(play, CardPlay::Cancelled);
    if !cancelled {
        resolved(play);
    }
    game.resume_internal_card().unwrap();
    !cancelled
}

fn evolve(game: &mut Game) {
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the actual base's intrinsic evolution resolves"
    );
    assert!(
        step.events.iter().any(|event| event["kind"] == "進化"
            && event["object"] == "source"
            && event["with"] == "form"),
        "a real evolution event produces the evolved trigger"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["evolve_deck"],
        json!([]),
        "the real evolved resource is consumed"
    );
}

#[test]
fn bp04_015_real_base_and_all_printings_evolve_into_phyto_healing_and_drawing() {
    for (base, form) in [
        ("BP04-015", "BP04-016"),
        ("SP01-006", "SP01-007"),
        ("SP01-SL06", "SP01-SL07"),
    ] {
        let mut game = game(&position(base, form));
        enter(&mut game);
        evolve(&mut game);
        assert!(trigger(&mut game, form, 2));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["hand"], json!(["first"]));
        assert_eq!(state["P1"]["deck"], json!(["second", "third"]));
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn bp05_030_damages_the_selected_leader_then_real_evolution_destroys_a_follower() {
    let mut game = game(&position("BP05-030", "BP05-031"));
    enter(&mut game);
    assert!(trigger(&mut game, "BP05-030", 2));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        16_u8
    );
    evolve(&mut game);
    assert!(trigger(&mut game, "BP05-031", 2));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 10_u8);
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
}

#[test]
fn cp02_072_real_evolution_requires_a_victim_before_destroying_and_self_damage() {
    for (base, form) in [("CP02-072", "CP02-073"), ("CP02-P57", "CP02-P58")] {
        for victim in [false, true] {
            let mut initial = position(base, form);
            if !victim {
                initial["players"]["P2"]["zones"]["field"] = json!([]);
            }
            let mut game = game(&initial);
            enter(&mut game);
            evolve(&mut game);
            assert_eq!(trigger(&mut game, form, 2), victim);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if victim { 7_u8 } else { 10 }
            );
            assert_eq!(
                state["P2"]["cemetery"],
                if victim { json!(["enemy"]) } else { json!([]) }
            );
            assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        }
    }
}

fn ward_counter(base: &str) {
    for acted in [false, true] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ward","card":base,"state":{"acted":acted}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":"P1.leader"})
            )
            .unwrap()
            .outcome,
            if acted { "cannot-attack" } else { "resolved" },
            "only an acted Ward protects the leader"
        );
    }
}

#[test]
fn bp20_064_awakening_only_changes_damage_then_real_evolution_keeps_ward() {
    for (base, form) in [("BP20-064", "BP20-065"), ("BP20-P33", "BP20-P34")] {
        ward_counter(base);
        ward_counter(form);
        for maximum in [6_u8, 7] {
            let mut initial = position(base, form);
            initial["players"]["P1"]["pp"] = json!({"max":maximum,"current":maximum});
            let mut game = game(&initial);
            enter(&mut game);
            assert!(trigger(&mut game, base, 2));
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["objects"]["enemy"]["damage"], u8::from(maximum == 7));
            assert_eq!(before["objects"]["source"]["damage"], 0_u8);
            assert_eq!(before["P1"]["leader"]["life"], 12_u8);
            evolve(&mut game);
            assert!(trigger(&mut game, form, 2));
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(
                after["objects"]["enemy"]["damage"],
                if maximum == 7 { 3_u8 } else { 2 }
            );
            assert_eq!(after["P1"]["pp"]["current"], maximum - 3);
            assert_eq!(after["P1"]["pp"]["max"], maximum);
        }
    }
}

#[test]
fn each_real_base_rejects_evolution_without_the_printed_one_pp() {
    for (base, form) in [
        ("BP04-015", "BP04-016"),
        ("BP05-030", "BP05-031"),
        ("CP02-072", "CP02-073"),
        ("BP20-064", "BP20-065"),
    ] {
        let mut initial = position(base, form);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
        let mut game = game(&initial);
        if let Ok(step) = game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}}),
        ) {
            assert_ne!(step.outcome, "resolved");
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["P1"]["evolve_deck"], json!(["form"]));
    }
}

#[test]
fn bp20_064_may_target_itself_when_awakened_before_real_evolution() {
    let mut game = game(&position("BP20-064", "BP20-065"));
    enter(&mut game);
    assert!(trigger(&mut game, "BP20-064", 1));
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["objects"]["source"]["damage"], 1_u8);
    assert_eq!(before["objects"]["enemy"]["damage"], 0_u8);
    assert_eq!(before["P1"]["leader"]["life"], 12_u8);
    evolve(&mut game);
    assert!(trigger(&mut game, "BP20-065", 2));
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(after["P1"]["pp"]["current"], 7_u8);
}
