use super::*;

fn position(card: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_i64);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-evolution-payment",
    )
    .unwrap()
}
fn evolve(game: &mut Game, costs: &Value) -> String {
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_i64},"costs":costs}),
    )
    .unwrap()
    .outcome
}
#[test]
fn bp01_105_evolution_pays_three_life_and_no_pp_or_ep() {
    for life in [2_u8, 20] {
        let mut initial = position("BP01-105", "BP01-106");
        initial["players"]["P1"]["leader"]["life"] = json!(life);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            evolve(&mut game, &json!({})),
            if life == 2 {
                "cannot-evolve"
            } else {
                "resolved"
            },
            "bp01 105 evolution pays three life and no pp or ep"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"], 10_i64,
            "bp01 105 evolution pays three life and no pp or ep"
        );
        assert_eq!(
            state["P1"]["ep"], before["P1"]["ep"],
            "bp01 105 evolution pays three life and no pp or ep"
        );
        if life == 2 {
            assert_eq!(
                state, before,
                "bp01 105 evolution pays three life and no pp or ep"
            );
        } else {
            assert_eq!(
                state["P1"]["leader"]["life"], 17_i64,
                "bp01 105 evolution pays three life and no pp or ep"
            );
            assert_eq!(
                state["objects"]["source"]["power"], 3_i64,
                "bp01 105 evolution pays three life and no pp or ep"
            );
            assert_eq!(
                state["objects"]["source"]["evolved_with"], "form",
                "bp01 105 evolution pays three life and no pp or ep"
            );
        }
    }
}
#[test]
fn bp04_094_evolution_banishes_exactly_two_own_cemetery_cards() {
    for available in [1_u8, 2] {
        let mut initial = position("BP04-094", "BP04-095");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..available)
                .map(|i| json!({"id":format!("fee{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            evolve(&mut game, &json!({"materials":["fee0","fee1"]})),
            if available == 1 {
                "cannot-evolve"
            } else {
                "resolved"
            },
            "bp04 094 evolution banishes exactly two own cemetery cards"
        );
        let state = game.projection(View::Referee).unwrap();
        if available == 1 {
            assert_eq!(
                state, before,
                "bp04 094 evolution banishes exactly two own cemetery cards"
            );
        } else {
            assert_eq!(
                state["P1"]["banish"],
                json!(["fee0", "fee1"]),
                "bp04 094 evolution banishes exactly two own cemetery cards"
            );
            assert_eq!(
                state["P1"]["cemetery"],
                json!([]),
                "bp04 094 evolution banishes exactly two own cemetery cards"
            );
            assert_eq!(
                state["objects"]["source"]["power"], 3_i64,
                "bp04 094 evolution banishes exactly two own cemetery cards"
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 10_i64,
                "bp04 094 evolution banishes exactly two own cemetery cards"
            );
        }
    }
}
#[test]
fn bp05_098_discards_one_own_hand_card_before_actual_evolution() {
    for zone in ["hand", "cemetery"] {
        let mut initial = position("BP05-098", "BP05-099");
        initial["players"]["P1"]["zones"][zone] = json!([{"id":"fee","card":"BP01-171"}]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            evolve(&mut game, &json!({"discard":["fee"]})),
            if zone == "hand" {
                "resolved"
            } else {
                "cannot-evolve"
            },
            "bp05 098 discards one own hand card before actual evolution"
        );
        let state = game.projection(View::Referee).unwrap();
        if zone == "cemetery" {
            assert_eq!(
                state, before,
                "bp05 098 discards one own hand card before actual evolution"
            );
        } else {
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["fee"]),
                "bp05 098 discards one own hand card before actual evolution"
            );
            assert_eq!(
                state["P1"]["hand"],
                json!([]),
                "bp05 098 discards one own hand card before actual evolution"
            );
            assert_eq!(
                state["objects"]["source"]["power"], 3_i64,
                "bp05 098 discards one own hand card before actual evolution"
            );
            assert_eq!(
                state["objects"]["source"]["evolved_with"], "form",
                "bp05 098 discards one own hand card before actual evolution"
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 10_i64,
                "bp05 098 discards one own hand card before actual evolution"
            );
        }
    }
}

#[test]
fn intrinsic_evolution_rejects_duplicate_wrong_count_and_source_materials_atomically() {
    let mut initial = position("BP04-094", "BP04-095");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"fee0","card":"BP01-171"},{"id":"fee1","card":"BP01-171"}]);
    for materials in [
        json!(["fee0"]),
        json!(["fee0", "fee0"]),
        json!(["source", "fee0"]),
    ] {
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            evolve(&mut game, &json!({"materials":materials})),
            "cannot-evolve",
            "intrinsic evolution rejects duplicate wrong count and source materials atomically"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "intrinsic evolution rejects duplicate wrong count and source materials atomically"
        );
    }
}

#[test]
fn bp04_095_evolved_form_grows_and_heals_at_its_controllers_end_phase() {
    let mut initial = position("BP04-094", "BP04-095");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"fee0","card":"BP01-171"},{"id":"fee1","card":"BP01-171"}]);
    let mut game = game(&initial);
    assert_eq!(
        evolve(&mut game, &json!({"materials":["fee0","fee1"]})),
        "resolved",
        "bp04 095 evolved form grows and heals at its controllers end phase"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed end phase trigger")
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["source"]["hp"], 4_i64,
        "bp04 095 evolved form grows and heals at its controllers end phase"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 21_i64,
        "bp04 095 evolved form grows and heals at its controllers end phase"
    );
}
