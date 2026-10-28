use super::*;
use alloc::collections::BTreeMap;
use sve_engine::ir::lowering::ExpandedLowerer;

fn tree() -> Value {
    let mut tree = fanfare::expanded::spell_tree().clone();
    tree["cards"]["SD03-005"]["abilities"][0]["spell"] = json!({
        "targets":[{"bind":"chosen","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":[
            {"set_stat":{"on":"chosen","power":1,"hp":1}},
            {"destroy":{"ref":"chosen"}},
            {"damage":{"to":{"leader_of":"chosen"},"n":{"at":"selection","selection":"chosen","value":{"hp_of":"chosen"}}}},
            {"damage":{"to":{"leader_of":"chosen"},"n":{"at":"selection","selection":"chosen","value":{"power_of":"chosen"}}}},
            {"damage":{"to":{"leader_of":"chosen"},"n":{"at":"resolution","value":{"count":{"who":"opp","zone":"cemetery"}}}}}
        ]
    });
    tree
}

#[test]
fn selection_captures_survive_stat_changes_and_departure_without_using_owner_or_opponent() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(&tree(), &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]))
        .unwrap();
    let (catalog, mut initial) =
        fanfare::fragment_setup(1, "SD03-005", 10, json!([{"id":"f","card":"BP01-037"}]));
    initial["players"]["P1"]["zones"]["deck"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"f","card":"BP01-037","owner":"P2","state":{"power":4,"damage":2}}]);
    let mut board = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "selection-captures",
    )
    .unwrap();
    board
        .load_internal_card("SD03-005", &cards["SD03-005"])
        .unwrap();
    board.load_internal_card("BP01-037", &card(vec![])).unwrap();
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["objects"]["f"]["power"], 4_u8);
    assert_eq!(before["objects"]["f"]["hp"], 3_u8);
    assert_eq!(before["P2"]["cemetery"], json!([]));
    let CardPlay::Awaiting(question) = board.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
    else {
        panic!("target");
    };
    resolved(
        board
            .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 13_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["f"]));
}

#[test]
fn selection_captures_reject_unknown_groups_wrong_anchors_and_multiple_targets() {
    let lowerer = ExpandedLowerer::new().unwrap();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    for (key, value) in [
        ("selection", json!("missing")),
        ("value", json!({"hp_of":"missing"})),
        ("at", json!("resolution")),
    ] {
        let mut invalid = tree();
        invalid["cards"]["SD03-005"]["abilities"][0]["spell"]["do"][2]["damage"]["n"][key] = value;
        lowerer
            .lower_with_printed_costs(&invalid, &costs)
            .unwrap_err();
    }
    let mut multiple = tree();
    multiple["cards"]["SD03-005"]["abilities"][0]["spell"]["targets"][0]["n"] = json!(2_u8);
    lowerer
        .lower_with_printed_costs(&multiple, &costs)
        .unwrap_err();
}
