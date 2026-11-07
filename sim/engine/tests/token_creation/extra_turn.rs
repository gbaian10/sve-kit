use super::*;
use sve_engine::ir::ScheduleToken;

fn selection(steps: &Value) -> (String, String, Value) {
    let (creator, source) = author(
        "creator",
        "11111111111111111111111111111111",
        "スペル",
        vec![json!({"spell":{"do":steps}})],
    );
    (
        format!("{creator}\n"),
        json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string(),
        json!({"documents":[],"authors":[source]}),
    )
}

#[expect(
    clippy::indexing_slicing,
    reason = "Fixed synthetic positions must fail if their shape changes."
)]
fn board(steps: &Value, seat: u32) -> Game {
    let (snapshot, keywords, input) = selection(steps);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let mut initial = initial(0);
    let actor = if seat == 1 { "P1" } else { "P2" };
    initial["turn"]["active"] = json!(actor);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["hand"] = json!([{"id":"creator","card":"creator"}]);
    Game::from_selected_effects(selected, &initial, &Value::Null, &Value::Null, "extra-turn")
        .unwrap()
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only schedule receipts belong to this extra-turn assertion."
)]
fn extra_turns_append_distinct_occurrences_without_advancing_the_current_turn() {
    for seat in [1_u32, 2] {
        let mut game = board(
            &json!([{ "extra_turn":{"who":"self"} },{ "extra_turn":{"who":"self"} }]),
            seat,
        );
        let CardPlay::Resolved(result) = game
            .play_internal_card("creator", PlayerId { seat })
            .unwrap()
        else {
            panic!("complete extra turn spell")
        };
        let tokens = result
            .progress
            .receipts
            .values()
            .filter_map(|outcome| match outcome {
                InstructionOutcome::ScheduledTurn(token) => Some(*token),
                _ => None,
            })
            .collect::<Vec<ScheduleToken>>();
        assert_eq!(tokens.len(), 2);
        assert_ne!(tokens.first(), tokens.get(1));
        let actor = if seat == 1 { "P1" } else { "P2" };
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["turn"]["active"], actor);
        assert_eq!(
            view["semantic_state"]["turn_schedule"]["extra"],
            json!([{"player":actor,"count":1_u8},{"player":actor,"count":1_u8}])
        );
    }
}

#[test]
fn a_later_invalid_instruction_rolls_back_the_scheduled_turn() {
    let mut game = board(
        &json!([{ "extra_turn":{"who":"self"} },{"life":{"who":"self","delta":{"times":[0x001f_ffff_ffff_ffff_i64,2_000_i64]}}}]),
        1,
    );
    let before = format!("{game:?}");
    game.play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(format!("{game:?}"), before);
}

#[test]
fn extra_turn_keeps_other_players_skip_and_unused_author_variants_closed() {
    for step in [
        json!({"extra_turn":{"who":"opp"}}),
        json!({"extra_turn":{"who":"self","result":"turn"}}),
        json!({"extra_turn":{"who":"self","n":1_u8}}),
        json!({"extra_turn":{"who":"self","precondition":true}}),
        json!({"skip_turn":{"who":"self"}}),
    ] {
        let (snapshot, keywords, input) = selection(&json!([step]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    }
}
