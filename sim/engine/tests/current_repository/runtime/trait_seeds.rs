use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

#[test]
fn dragoweapon_selects_before_paying_and_adds_health_and_armament() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"weapon","card":"BP02-T07"},
        {"id":"dragon","card":"BP01-092"}
    ]);
    let mut board = game(&initial);
    let before = board.projection(View::Referee).unwrap()["objects"]["dragon"]["hp"]
        .as_i64()
        .unwrap();
    let CardPlay::Awaiting(question) = board
        .activate_internal_card("weapon", AbilityId(1), actor())
        .unwrap()
    else {
        panic!("target");
    };
    assert!(matches!(question.domain, InputDomain::Targets(_)));
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["weapon", "dragon"])
    );
    resolved(
        board
            .answer_internal_card(actor(), question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["dragon"]["hp"], before + 1);
    assert!(
        state["objects"]["dragon"]["traits"]
            .as_array()
            .unwrap()
            .contains(&json!("武装"))
    );
    assert!(
        !state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("weapon"))
    );
}

#[test]
fn draconic_armor_creates_two_complete_weapons_while_awakened() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP02-068"}]);
    let mut board = game(&initial);
    resolved(board.play_internal_card("source", actor()).unwrap());
    let state = board.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 2);
    for id in field {
        assert_eq!(state["objects"][id.as_str().unwrap()]["card"], "BP02-T07");
    }
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn dragon_smash_damages_the_target_and_creates_a_complete_weapon() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-066"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut board = game(&initial);
    let CardPlay::Awaiting(question) = board.play_internal_card("source", actor()).unwrap() else {
        panic!("target");
    };
    resolved(
        board
            .answer_internal_card(actor(), question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 7_u8);
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 1);
    assert_eq!(
        state["objects"][field[0].as_str().unwrap()]["card"],
        "BP02-T07"
    );
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}
