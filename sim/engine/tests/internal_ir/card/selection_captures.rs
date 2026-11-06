use super::*;
use alloc::collections::BTreeMap;

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
    let cards = lowerer()
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
    let lowerer = lowerer();
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

#[test]
fn selection_captures_count_distinct_original_costs_of_the_whole_public_selection() {
    for (members, expected) in [(vec![0, 1], 19_u8), (vec![0, 2], 18)] {
        let mut tree = tree();
        tree["cards"]["SD03-005"]["abilities"][0]["spell"] = json!({
            "targets":[{"bind":"chosen","from":{"who":"self","zone":"field","kind":"follower"},"n":2}],
            "do":[{"damage":{"to":{"leader":"self"},"n":{
                "at":"selection","selection":"chosen","value":{"distinct_orig_costs":{"in":"chosen"}}
            }}}]
        });
        let cards = lowerer()
            .lower_with_printed_costs(&tree, &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]))
            .unwrap();
        let (catalog, mut initial) = fanfare::fragment_setup(
            1,
            "SD03-005",
            10,
            json!([{ "id": "catalog-a", "card": "BP01-037" }, { "id": "catalog-b", "card": "BP01-118" }]),
        );
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"a","card":"BP01-037","state":{"cost":9}},
            {"id":"b","card":"BP01-037","state":{"cost":0}},
            {"id":"c","card":"BP01-118"}
        ]);
        let mut board = Game::new(
            catalog,
            &initial,
            &Value::Null,
            &Value::Null,
            "selected-costs",
        )
        .unwrap();
        board
            .load_internal_card("SD03-005", &cards["SD03-005"])
            .unwrap();
        for number in ["BP01-037", "BP01-118"] {
            board.load_internal_card(number, &card(vec![])).unwrap();
        }
        let CardPlay::Awaiting(question) =
            board.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
        else {
            panic!("two public objects");
        };
        resolved(
            board
                .answer_internal_card(question.actor, question.token, Answer::Targets(members))
                .unwrap(),
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            expected
        );
    }
}

#[test]
fn source_link_history_rejects_other_recipients_sources_windows_and_movement_events() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut observer = tree["cards"]["BP01-041"].clone();
    let mut ability = observer["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("activate");
    ability["auto"] = json!({"on":{"moved":{
        "from":{"who":"opp","zone":"field"},"to":"cemetery","during":{"turn":"self"},
        "sel":{"kind":"follower","where":{"history":{
            "event":"dealt_damage","window":"this_turn","who":"self","sel":{"trait":"ウマ娘"},"where":{"recipient":"item"}
        }}}
    }},"do":[{"draw":{"n":1}}]});
    observer["abilities"] = json!([ability]);
    tree["cards"] = json!({"observer":observer});
    lowerer().lower(&tree).unwrap();
    for (path, value) in [
        (
            "/cards/observer/abilities/0/auto/on/moved/sel/where/history/where/recipient",
            json!("this"),
        ),
        (
            "/cards/observer/abilities/0/auto/on/moved/sel/where/history/who",
            json!("opp"),
        ),
        (
            "/cards/observer/abilities/0/auto/on/moved/sel/where/history/window",
            json!("game"),
        ),
        (
            "/cards/observer/abilities/0/auto/on/moved/to",
            json!("hand"),
        ),
        (
            "/cards/observer/abilities/0/auto/on/moved/sel",
            json!({"kind":"follower","trait":"extra","where":{"history":{
                "event":"dealt_damage","window":"this_turn","who":"self","sel":{"trait":"ウマ娘"},"where":{"recipient":"item"}
            }}}),
        ),
    ] {
        let mut invalid = tree.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert_eq!(
            lowerer().lower(&invalid).unwrap_err().code,
            "unsupported_construct"
        );
    }
    tree["cards"]["observer"]["abilities"][0]["auto"]["on"]["moved"]["sel"]["where"]["history"] = json!({
        "event":"dealt_damage","window":"this_turn","where":{"ref":"this","recipient":"item"}
    });
    lowerer().lower(&tree).unwrap();
}

#[test]
fn selected_cost_collections_reject_unknown_groups_private_choices_and_wrong_anchors() {
    let mut candidate = tree();
    candidate["cards"]["SD03-005"]["abilities"][0]["spell"]["do"] = json!([
        {"damage":{"to":{"leader":"self"},"n":{"at":"selection","selection":"chosen","value":{"distinct_orig_costs":{"in":"chosen"}}}}}
    ]);
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    lowerer()
        .lower_with_printed_costs(&candidate, &costs)
        .unwrap();
    for (path, value) in [
        (
            "/cards/SD03-005/abilities/0/spell/do/0/damage/n/selection",
            json!("missing"),
        ),
        (
            "/cards/SD03-005/abilities/0/spell/do/0/damage/n/value/distinct_orig_costs/in",
            json!("missing"),
        ),
        (
            "/cards/SD03-005/abilities/0/spell/do/0/damage/n/at",
            json!("resolution"),
        ),
        (
            "/cards/SD03-005/abilities/0/spell/targets/0/from/zone",
            json!("hand"),
        ),
    ] {
        let mut invalid = candidate.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        lowerer()
            .lower_with_printed_costs(&invalid, &costs)
            .unwrap_err();
    }
}
