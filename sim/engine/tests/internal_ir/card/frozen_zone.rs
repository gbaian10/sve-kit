#![expect(
    clippy::panic,
    reason = "Unexpected fixture decisions fail the timing scenario."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

fn timing_game(condition: &Value, modes: bool) -> Game {
    let mill = json!({"move":{"top":{"who":"self","n":1},"to":"cemetery"}});
    let conditional = json!({"if":{"cond":condition,"then":[{"life":{"who":"self","delta":2}}]}});
    let steps = if modes {
        json!([{"choice":{"n":2,"options":[[mill],[conditional]]}}])
    } else {
        json!([mill, conditional])
    };
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["BP01-118"]["abilities"][0]["fanfare"] = json!({"do":steps});
    let lowered = lowerer().lower(&tree).unwrap();
    let (catalog, mut initial) =
        fanfare::fragment_setup(1, "BP01-118", 10, json!([{"id":"top","card":"BP01-065"}]));
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..9_u8)
            .map(|i| json!({"id":format!("grave-{i}"),"card":"BP01-065"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "frozen-zone-timing",
    )
    .unwrap();
    game.load_internal_card("BP01-118", &lowered["BP01-118"])
        .unwrap();
    game.load_internal_card("BP01-065", &card(vec![])).unwrap();
    assert!(
        matches!(
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ),
        "the ordinary fanfare triggers before its body or mode choice"
    );
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("fanfare")
    };
    assert_eq!(
        options.len(),
        1,
        "the source fanfare is the only pending ability"
    );
    let mut play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    if let CardPlay::Awaiting(question) = play {
        assert!(
            matches!(question.domain, InputDomain::Modes { .. }),
            "only the two printed modes need an answer"
        );
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(1), ModeId(2)]),
            )
            .unwrap();
    }
    resolved(play);
    game
}

#[test]
fn sc_and_nc_freeze_before_earlier_steps_or_modes_change_the_cemetery_while_ordinary_counts_stay_live()
 {
    for modes in [false, true] {
        for (condition, life) in [
            (json!({"spellchain":10}), 10_u8),
            (json!({"necro":10}), 10_u8),
            (
                json!({"cmp":{"left":{"count":{"who":"self","zone":"cemetery"}},"op":"ge","right":10}}),
                12_u8,
            ),
        ] {
            let game = timing_game(&condition, modes);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["cemetery"].as_array().unwrap().len(),
                10_usize,
                "the earlier mill step really adds the tenth card"
            );
            assert_eq!(
                state["P1"]["leader"]["life"], life,
                "SC/NC retain the initial nine, while ordinary C.live sees ten"
            );
        }
    }
}
