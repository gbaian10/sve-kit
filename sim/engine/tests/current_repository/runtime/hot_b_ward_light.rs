use super::*;

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":number,"state":{"entered_this_turn":false}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"guard","card":"BP01-T09","state":{"acted":true}},
        {"id":"enemy","card":"BP01-037","state":{"hp":20_u8}}
    ]);
    initial
}

fn attack(board: &mut Game, target: &str) -> String {
    board
        .internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
        .unwrap()
        .outcome
}

#[test]
fn ice_warrior_printings_attack_the_leader_past_a_rested_ward() {
    for number in ["BP06-064", "BP06-P18"] {
        let mut board = game(&position(number));
        assert_eq!(attack(&mut board, "P2.leader"), "resolved");
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["field"], json!(["guard", "enemy"]));
    }
}

#[test]
fn wandering_elf_assail_attacks_a_standing_nonward_past_a_rested_ward() {
    let mut board = game(&position("SD01-013"));
    assert_eq!(attack(&mut board, "enemy"), "resolved");
    assert!(
        board.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃"))
    );
}
