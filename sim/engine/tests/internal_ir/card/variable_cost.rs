use super::*;

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
