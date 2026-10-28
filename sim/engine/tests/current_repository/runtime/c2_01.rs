#![expect(
    clippy::panic,
    reason = "An unexpected decision domain means the game scenario is malformed."
)]

use super::*;

fn trial_game(initial: &Value) -> Game {
    let selected = selection_with_rules().clone();
    for card in ["CP03-016", "BP01-131", "SD03-005", "BP20-P38"] {
        assert!(
            selected.current.contains_key(card),
            "{card} uses current DSL"
        );
    }
    Game::from_selected_effects(selected, initial, &Value::Null, &Value::Null, "c2-01").unwrap()
}

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: seat_number(seat),
    }
}

fn pending_card(game: &mut Game, card: &str) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending ability");
    };
    let [option] = options.as_slice() else {
        panic!("one pending ability for this player");
    };
    assert_eq!(
        option.card, card,
        "the expected card owns this pending ability"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap(),
    )
}

#[test]
fn cp03_016_fanfare_draws_once_for_its_controller_and_keeps_the_card_private() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"played","card":"CP03-016"}]);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"watcher","card":"CP03-016"}]);
        initial["players"][opponent]["zones"]["field"] = json!([{"id":"enemy","card":"CP03-016"}]);
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":"private-draw","card":"SD03-005"},
            {"id":"remaining","card":"SD03-005"}
        ]);
        let mut game = trial_game(&initial);
        assert!(matches!(
            game.play_internal_card("played", actor(seat)).unwrap(),
            CardPlay::Entered(_)
        ));
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before[seat]["hand"], json!([]));
        assert_eq!(before[seat]["pp"]["current"], 8_u64);
        pending_card(&mut game, "CP03-016");
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["hand"], json!(["private-draw"]));
        assert_eq!(state[seat]["deck"], json!(["remaining"]));
        assert_eq!(state[opponent]["hand"], json!([]));
        let private = game
            .projection(if seat == "P1" { View::P2 } else { View::P1 })
            .unwrap();
        assert!(!private.to_string().contains("private-draw"));
    }
}

#[test]
fn cp03_016_failed_payment_neither_enters_nor_draws() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":1_u64,"max":1_u64});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"played","card":"CP03-016"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d","card":"SD03-005"}]);
    let mut game = trial_game(&initial);
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("played", actor("P1")).unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn bp01_131_destroys_both_fields_then_resolves_each_lastword_once() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-131"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"own-lastword","card":"BP01-P25"},
        {"id":"own-follower","card":"CP03-016"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy-lastword","card":"BP01-P25"}]);
    let mut game = trial_game(&initial);
    resolved(game.play_internal_card("spell", actor("P1")).unwrap());
    let before_lastwords = game.projection(View::Referee).unwrap();
    assert_eq!(before_lastwords["P1"]["pp"]["current"], 5_u64);
    assert_eq!(before_lastwords["P1"]["field"], json!([]));
    assert_eq!(before_lastwords["P2"]["field"], json!([]));
    assert_eq!(before_lastwords["P1"]["leader"]["life"], 20_u64);
    assert_eq!(before_lastwords["P2"]["leader"]["life"], 20_u64);
    assert!(
        before_lastwords["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("own-lastword"))
    );
    assert_eq!(
        before_lastwords["P2"]["cemetery"],
        json!(["enemy-lastword"])
    );
    game.resume_internal_card().unwrap();
    pending_card(&mut game, "BP01-P25");
    game.resume_internal_card().unwrap();
    pending_card(&mut game, "BP01-P25");
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 16_u64);
    assert_eq!(state["P2"]["leader"]["life"], 16_u64);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["deck"], json!([]));
}

#[test]
fn bp01_131_can_resolve_without_followers_but_failed_payment_is_atomic() {
    for pp in [4_u8, 5_u8] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-131"}]);
        let mut game = trial_game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let result = game.play_internal_card("spell", actor("P1"));
        if pp == 4 {
            result.unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        } else {
            resolved(result.unwrap());
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 0_u64);
            assert_eq!(state["P1"]["cemetery"], json!(["spell"]));
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn sd03_005_quick_draw_can_play_the_next_quick_in_the_same_end_phase() {
    let mut initial = draw_position();
    initial["players"]["P2"]["pp"] = json!({"current":2_u64,"max":2_u64});
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"first","card":"SD03-005"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"second","card":"SD03-005"},
        {"id":"private-draw","card":"CP03-016"}
    ]);
    let mut game = trial_game(&initial);
    let before_window = game.projection(View::Referee).unwrap();
    game.play_internal_card("first", actor("P2")).unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before_window);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    for card in ["first", "second"] {
        let question = game.internal_quick_choice().unwrap().unwrap();
        assert_eq!(question.actor, actor("P2"));
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick window");
        };
        let [option] = options.as_slice() else {
            panic!("one legal Quick spell")
        };
        assert_eq!(option.card, "SD03-005");
        assert!(game.projection(View::P1).unwrap()["awaiting"]["domain"].is_null());
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            actor("P1"),
            question.token,
            Answer::Quick(Some(option.handle)),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(option.handle)),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!(card))
        );
    }
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert!(matches!(&question.domain, InputDomain::Quick { options, .. } if options.is_empty()));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["pp"]["current"], 0_u64);
    assert_eq!(state["P2"]["hand"], json!(["private-draw"]));
    assert_eq!(state["P2"]["deck"], json!([]));
    assert!(
        !game
            .projection(View::P1)
            .unwrap()
            .to_string()
            .contains("private-draw")
    );
    assert!(matches!(
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap(),
        CardPlay::Continued(_)
    ));
}

#[test]
fn sd03_005_main_play_draws_once_and_insufficient_pp_preserves_hand_and_deck() {
    for pp in [0_u8, 1_u8] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":1_u64});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"SD03-005"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"drawn","card":"CP03-016"},
            {"id":"remaining","card":"CP03-016"}
        ]);
        let mut game = trial_game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let result = game.play_internal_card("spell", actor("P1"));
        if pp == 0 {
            result.unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        } else {
            resolved(result.unwrap());
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 0_u64);
            assert_eq!(state["P1"]["hand"], json!(["drawn"]));
            assert_eq!(state["P1"]["deck"], json!(["remaining"]));
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn bp20_p38_zero_pp_activation_targets_only_allies_and_rejects_repeat_use() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["pp"] = json!({"current":0_u64,"max":0_u64});
        initial["players"][seat]["zones"]["field"] = json!([{"id":"source","card":"BP20-P38"}]);
        initial["players"][opponent]["zones"]["field"] = json!([{"id":"enemy","card":"CP03-016"}]);
        let mut game = trial_game(&initial);
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("source", AbilityId(1), actor(seat))
            .unwrap()
        else {
            panic!("target choice");
        };
        let InputDomain::Targets(targets) = &question.domain else {
            panic!("target choice")
        };
        let [target] = targets.members.as_slice() else {
            panic!("only the allied source is legal")
        };
        assert_eq!(target.card.as_deref(), Some("BP20-P38"));
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![99]))
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![target.handle]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["pp"]["current"], 0_u64);
        assert_eq!(state["objects"]["source"]["hp"], 2_u64);
        assert_eq!(state["objects"]["enemy"]["hp"], 3_u64);
        let after_damage = state;
        game.activate_internal_card("source", AbilityId(1), actor(seat))
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), after_damage);
    }
}

#[test]
fn bp20_p38_lethal_self_damage_leaves_the_field_without_triggering_entry_draw() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":0_u64,"max":0_u64});
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP20-P38","state":{"hp":1_u64}}]);
    let mut game = trial_game(&initial);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", AbilityId(1), actor("P1"))
        .unwrap()
    else {
        panic!("target choice")
    };
    let InputDomain::Targets(targets) = &question.domain else {
        panic!("target choice")
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![targets.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 0_u64);
    assert!(game.internal_pending_choice().unwrap().is_none());
}
