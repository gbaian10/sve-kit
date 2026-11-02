use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

#[test]
fn bp03_057_assail_can_attack_a_standing_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP03-056","state":{"evolved":true,"evolved_with":"form","power":6_u8,"hp":6_u8,"max_hp":6_u8}}]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"BP03-057"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["objects"]["enemy"]["acted"], false);
    assert!(
        before["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃"))
    );
    board
        .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    board
        .answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["field"],
        json!([])
    );
}

#[test]
fn bp03_060_fanfare_creates_a_complete_weapon() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-060"}]);
    let mut board = game(&initial);
    assert!(matches!(
        board.play_internal_card("source", actor()).unwrap(),
        CardPlay::Entered(_)
    ));
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP03-060");
    resolved(
        board
            .answer_internal_card(actor(), question.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    let weapons = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP02-T07")
        .count();
    assert_eq!(weapons, 1);
}

#[test]
fn bp03_072_moves_its_armed_ally_to_ex_damages_and_creates_a_weapon() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-072"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP03-060"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut board = game(&initial);
    let mut play = board.play_internal_card("source", actor()).unwrap();
    for _ in 0..2_usize {
        let CardPlay::Awaiting(question) = play else {
            panic!("formal target");
        };
        assert!(matches!(question.domain, InputDomain::Targets(_)));
        play = board
            .answer_internal_card(actor(), question.token, Answer::Targets(vec![0]))
            .unwrap();
    }
    resolved(play);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["ally"]));
    assert_eq!(state["objects"]["enemy"]["hp"], 7_u8);
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 1);
    assert_eq!(
        state["objects"][field[0].as_str().unwrap()]["card"],
        "BP02-T07"
    );
}
