use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{pending, question};
use super::*;

fn position() -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["leader"]["class"] = json!("エルフ");
    value["players"]["P1"]["construction"] = json!("title");
    value["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    value["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"nea","card":"CP04-012"}]);
    value["players"]["P2"]["zones"]["hand"] = json!([]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"gil","card":"BP01-158"}]);
    for side in ["P1", "P2"] {
        value["players"][side]["zones"]["deck"] = json!(
            (0..6_u8)
                .map(|n| json!({"id":format!("{side}-d{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
    }
    value
}

fn loaded(value: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        value,
        &Value::Null,
        &Value::Null,
        "nea",
    )
    .unwrap()
}

fn activate(board: &mut Game, holder: &str) {
    resolved(
        board
            .activate_internal_card(holder, AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    finish_checks(board);
}

fn finish_turn(board: &mut Game) {
    let result = board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    assert_eq!(
        result.outcome, "resolved",
        "a legal turn end advances to the Quick window"
    );
    finish_checks(board);
    let q = board.internal_quick_choice().unwrap().unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish_checks(board);
    board
        .internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    finish_checks(board);
}

#[test]
fn nea_activates_by_acting_and_requires_one_attack_on_only_the_next_opponent_turn() {
    let mut board = loaded(&position());
    activate(&mut board, "nea");
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["nea"]["acted"],
        true
    );
    finish_turn(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "cannot-play"
    );
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"gil","target":"P1.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Nea's printed Ward still requires a follower target"
    );
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"gil","target":"nea"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    finish_checks(&mut board);
    let quick = board.internal_quick_choice().unwrap().unwrap();
    board
        .answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
        .unwrap();
    finish_checks(&mut board);
    let q = question(pending(&mut board, "CP04-012"));
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("lastword targets");
    };
    resolved(
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    );
    finish_checks(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["nea"])
    );
    finish_turn(&mut board);
    finish_turn(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["gil"]["acted"],
        true,
        "Lastword prevents the target's next rule stand"
    );
}
