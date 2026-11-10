#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed evolution sequence."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn evolve(base: &str, form: &str, super_evolution: bool) -> (Game, Value) {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    initial["players"]["P1"]["leader"]["class"] = json!("エルフ");
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base},{"id":"field_fairy","card":"BP01-T03"},
        {"id":"field_puppet","card":"BP08-T09"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"ex_fairy","card":"BP01-T03"},{"id":"ex_puppet","card":"BP08-T09"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opponent","card":"BP01-T03"}]);
    let mut board = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "r7a-fairy",
    )
    .unwrap();
    let mut action =
        json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}});
    if super_evolution {
        action["pay"]["sep"] = json!(1_u8);
    }
    assert_eq!(
        board.internal_rule_action(&action).unwrap().outcome,
        "resolved",
        "the printed evolution action resolves before its triggered abilities"
    );
    let before = board.projection(View::Referee).unwrap();
    (board, before)
}

fn resolve_line(board: &mut Game, number: &str, line: u64) {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("evolution trigger")
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(option.handle),
            )
            .unwrap(),
    );
    finish_checks(board);
}

fn stats(state: &Value, id: &str, power: u8, hp: u8) {
    assert_eq!(
        state["objects"][id]["power"], power,
        "the printed numeric modification affects attack"
    );
    assert_eq!(
        state["objects"][id]["hp"], hp,
        "the printed numeric modification affects life"
    );
}

fn created_fairy(state: &Value) -> &str {
    state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .find(|id| !["source", "field_fairy", "field_puppet"].contains(id))
        .unwrap()
}

fn unaffected(state: &Value, before: &Value) {
    for id in ["source", "field_puppet", "ex_puppet", "opponent"] {
        for stat in ["power", "hp"] {
            assert_eq!(
                state["objects"][id][stat], before["objects"][id][stat],
                "non-token Aria, other tokens and opposing followers do not receive this buff"
            );
        }
    }
}

#[test]
fn aria_all_prints_normal_evolution_creates_a_fairy_without_the_super_buff() {
    for (base, form) in [
        ("BP16-001", "BP16-002"),
        ("BP16-SL01", "BP16-SL02"),
        ("EBD01-001", "EBD01-002"),
    ] {
        let (mut board, before) = evolve(base, form, false);
        resolve_line(&mut board, form, 1);
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "ordinary evolution has only the first printed trigger"
        );
        let state = board.projection(View::Referee).unwrap();
        stats(&state, "field_fairy", 1, 1);
        stats(&state, "ex_fairy", 1, 1);
        stats(&state, created_fairy(&state), 1, 1);
        unaffected(&state, &before);
    }
}

#[test]
fn aria_all_prints_super_evolution_buffs_both_zones_and_allows_either_trigger_order() {
    for (base, form) in [
        ("BP16-001", "BP16-002"),
        ("BP16-SL01", "BP16-SL02"),
        ("EBD01-001", "EBD01-002"),
    ] {
        for order in [[1, 2], [2, 1]] {
            let (mut board, before) = evolve(base, form, true);
            resolve_line(&mut board, form, order[0]);
            resolve_line(&mut board, form, order[1]);
            assert!(board.internal_pending_choice().unwrap().is_none());
            let state = board.projection(View::Referee).unwrap();
            stats(&state, "field_fairy", 2, 2);
            stats(&state, "ex_fairy", 2, 2);
            let created_stat = if order[0] == 1 { 2 } else { 1 };
            stats(&state, created_fairy(&state), created_stat, created_stat);
            unaffected(&state, &before);
        }
    }
}

#[test]
fn arias_ex_numeric_modification_survives_the_fairys_later_field_entry() {
    let (mut board, _) = evolve("BP16-001", "BP16-002", true);
    resolve_line(&mut board, "BP16-002", 2);
    resolve_line(&mut board, "BP16-002", 1);
    assert!(matches!(
        board
            .play_internal_card("ex_fairy", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish_checks(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("ex_fairy"))
    );
    assert_eq!(state["P1"]["ex"], json!(["ex_puppet"]));
    stats(&state, "ex_fairy", 2, 2);
}
