#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-171"},
            {"id":format!("{seat}-second"),"card":"BP01-171"}
        ]);
    }
    initial
}

fn quick_pass(game: &mut Game) {
    for _ in 0_u8..4 {
        if game.internal_pending_choice().unwrap().is_some() {
            return;
        }
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("The Quick window must finish");
}

#[test]
fn sd06_009_adds_one_to_its_retaliation_damage_on_the_opposing_turn() {
    let mut initial = position("SD06-009");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"SD06-009","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"source"}))
            .unwrap()
            .outcome,
        "resolved",
        "The opposing Sage can attack this acted Ward"
    );
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}
