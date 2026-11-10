#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the complete Chloe sequence."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn pending_card(board: &mut Game, number: &str) -> CardPlay {
    finish_checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending ability")
    };
    assert_eq!(
        options.len(),
        1_usize,
        "one committed event creates one pending occurrence"
    );
    assert_eq!(
        options[0].card, number,
        "the actual printing owns its pending ability"
    );
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

fn target(board: &mut Game, play: CardPlay) -> Activation {
    let CardPlay::Awaiting(q) = play else {
        panic!("formal target")
    };
    let InputDomain::Targets(domain) = q.domain else {
        panic!("formal target domain")
    };
    resolved(
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    )
}

#[test]
fn r7b_chloe_all_prints_resolve_ub_and_observe_a_princess_spell_after_its_body() {
    for number in ["CP04-048", "CP04-P36", "PR-496"] {
        assert!(selection_with_rules().current.contains_key(number));
        let mut initial = draw_position();
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"friend","card":"CP04-047"}]);
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"chloe","card":number},{"id":"spell","card":"CP04-053"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}]);
        let mut board = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "r7b-chloe",
        )
        .unwrap();
        assert!(matches!(
            board
                .play_internal_card("chloe", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let fanfare = pending_card(&mut board, number);
        let result = target(&mut board, fanfare);
        assert!(result.progress.facts.iter().any(|fact| matches!(
            fact,
            Fact::AbilityPlayed {
                qualification: AbilityQualification::UnionBurst,
                ..
            }
        )));
        finish_checks(&mut board);
        let fanfare_state = board.projection(View::Referee).unwrap();
        assert_eq!(fanfare_state["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(
            fanfare_state["P2"]["leader"]["life"], 20_u8,
            "Chloe does not observe her own entry"
        );
        let spell = board
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap();
        target(&mut board, spell);
        let triggered = pending_card(&mut board, number);
        resolved(triggered);
        finish_checks(&mut board);
        assert!(board.internal_pending_choice().unwrap().is_none());
        let final_state = board.projection(View::Referee).unwrap();
        assert_eq!(final_state["objects"]["enemy"]["damage"], 5_u8);
        assert_eq!(final_state["P2"]["leader"]["life"], 19_u8);
        assert_eq!(final_state["P1"]["hand"], json!(["drawn"]));
    }
}

#[test]
fn r7b_chloe_without_an_enemy_cannot_activate_ub() {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"chloe","card":"CP04-048"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut board = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "r7b-chloe-no-target",
    )
    .unwrap();
    assert!(matches!(
        board
            .play_internal_card("chloe", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    assert_eq!(pending_card(&mut board, "CP04-048"), CardPlay::Cancelled);
    assert_eq!(
        board.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]["ub_activated"],
        0_u8
    );
}
