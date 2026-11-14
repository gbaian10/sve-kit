use super::*;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::QuoteBasis;

fn program(play: &Value) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["play"] = play.clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"unit-follower":seed});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn position(zone: &str) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"s","card":"unit-follower"}]);
    initial
}

fn from_position(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        catalog_with_cost(&json!("3")),
        initial,
        &Value::Null,
        &Value::Null,
        "variable-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game
}

fn play(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ),
        "normal follower payment and entry complete"
    );
}

#[test]
fn named_count_discounts_use_own_public_cards_and_floor_only_the_total() {
    let program = program(
        &json!({"define":{"X":{"count":{"who":"self","zone":"cemetery","kind":"spell"}}},"cost_delta":{"minus":[0,"X"]}}),
    );
    for count in [0_u8, 2, 5] {
        let mut initial = position("hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("own-{i}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"opponent","card":"unit-spell"}]);
        let mut game = from_position(&initial, &program);
        let quoted = cost_observation::preview(&game, Viewer::P1);
        assert_eq!(quoted.cost.basis, QuoteBasis::CurrentContext);
        assert_eq!(
            quoted.cost.total_pp,
            Some((3_i64 - i64::from(count)).max(0))
        );
        assert!(
            game.observe(Viewer::P2).unwrap().costs.cards.is_empty(),
            "opponents never receive the private card quote"
        );
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            match count {
                0 => 7_u8,
                2 => 9,
                _ => 10,
            }
        );
    }
}

#[test]
fn positive_variable_cost_counts_both_fields_and_excludes_the_pending_follower() {
    let program =
        program(&json!({"cost_delta":{"count":{"who":"all","zone":"field","kind":"follower"}}}));
    for count in [0_u8, 2] {
        let mut initial = position("hand");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("own-{i}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut game = from_position(&initial, &program);
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8 - count
        );
    }
}

#[test]
fn source_zone_condition_survives_resolution_and_composes_with_public_existence() {
    let program = program(
        &json!({"cost_delta":{"if":{"all":[{"in_zone":{"ref":"this","zone":"ex"}},{"exists":{"who":"self","zone":"field","kind":"follower"}}]},"delta":-2_i8}}),
    );
    for zone in ["hand", "ex"] {
        for present in [false, true] {
            let mut initial = position(zone);
            if present {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"own","card":"unit-follower"}]);
            }
            let mut game = from_position(&initial, &program);
            play(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if zone == "ex" && present { 9_u8 } else { 7_u8 }
            );
        }
    }
}

#[test]
fn variable_cost_does_not_authorize_filtered_hidden_information() {
    let mut tree = fanfare::expanded::tree().clone();
    let entry = &mut tree["cards"]["BP08-038"]["abilities"][0];
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["play"] =
        json!({"cost_delta":{"minus":[0,{"count":{"who":"opp","zone":"hand","kind":"spell"}}]}});
    lowerer().lower(&tree).unwrap_err();
}

#[test]
fn own_banish_zone_size_discounts_without_reading_the_opponent_zone() {
    let program =
        program(&json!({"cost_delta":{"minus":[0,{"count":{"who":"self","zone":"banish"}}]}}));
    for count in [0_u8, 2, 5] {
        let mut initial = position("hand");
        initial["players"]["P1"]["zones"]["banish"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("own-{i}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["banish"] =
            json!([{"id":"opponent","card":"unit-spell"}]);
        let mut game = from_position(&initial, &program);
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            match count {
                0 => 7_u8,
                2 => 9,
                _ => 10,
            }
        );
    }
}

#[test]
fn filtered_banish_zone_counts_stay_closed_for_play_costs() {
    let mut tree = fanfare::expanded::tree().clone();
    let entry = &mut tree["cards"]["BP08-038"]["abilities"][0];
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["play"] =
        json!({"cost_delta":{"minus":[0,{"count":{"who":"self","zone":"banish","kind":"spell"}}]}});
    lowerer().lower(&tree).unwrap_err();
}

#[test]
fn played_history_controls_only_the_first_matching_card_discount() {
    let mut expanded = fanfare::expanded::tree().clone();
    let mut seed = expanded["cards"]["BP08-038"].clone();
    let mut header = seed["abilities"][0].clone();
    header.as_object_mut().unwrap().remove("fanfare");
    header["static"] = json!({
        "while":{"cmp":{"left":{"history_count":{"event":"played","who":"self","window":"this_turn","sel":{"kind":"follower"}}},"op":"eq","right":0}},
        "effects":[{"cost_mod":{"when_playing":{"who":"self","kind":"follower"},"delta":-1}}]
    });
    seed["abilities"] = json!([header]);
    expanded["cards"] = json!({"unit-amulet":seed});
    let program = lowerer()
        .lower(&expanded)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"provider","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"unit-follower"},{"id":"second","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog_with_cost(&json!("3")),
        &initial,
        &Value::Null,
        &Value::Null,
        "played-cost",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    for (id, remaining) in [("first", 8_u8), ("second", 5_u8)] {
        assert!(matches!(
            game.play_internal_card(id, PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ));
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            remaining
        );
    }
}
