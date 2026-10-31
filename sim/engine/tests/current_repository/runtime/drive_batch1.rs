use super::*;

fn position(number: &str) -> Value {
    let mut initial = drive::position(number, number, 3);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([]);
    initial
}

fn single(game: &mut Game, number: &str) {
    let result = resolved(drive::attack(game, number));
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::DriveChecked(_)))
            .count(),
        1_usize,
        "Single Drive completes exactly one actual revealed check"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["second", "third", "tail", "first"]),
        "one check moves only the initial top to the bottom"
    );
}

#[test]
fn cp03_086_and_all_printings_mill_two_on_entry_and_have_one_single_drive_check() {
    for number in ["CP03-086", "CP03-SL18", "PR-281"] {
        let mut initial = position(number);
        let mut game = drive::drive_game(&initial, &Value::Null);
        single(&mut game, number);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        let mut played = drive::drive_game(&initial, &Value::Null);
        assert!(matches!(
            played
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let q = played.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed Fanfare")
        };
        resolved(
            played
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        let view = played.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["cemetery"], json!(["first", "second"]));
        assert_eq!(view["P1"]["deck"], json!(["third", "tail"]));
        assert_eq!(view["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn csd03a_003_and_all_printings_drive_once_but_do_not_evolve_from_normal_play() {
    for number in ["CSD03a-003", "CSD03a-SL03"] {
        let mut game = drive::drive_game(&position(number), &Value::Null);
        single(&mut game, number);
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"CSD03a-004"}]);
        let mut played = drive::drive_game(&initial, &Value::Null);
        assert!(matches!(
            played
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let q = played.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed conditional Fanfare")
        };
        resolved(
            played
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        let view = played.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["evolve_deck"],
            json!(["form"]),
            "normal play cannot satisfy ability entry"
        );
        assert_eq!(view["objects"]["source"]["evolved_with"], Value::Null);
        assert_eq!(view["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn csd03a_004_and_all_printings_evolve_normally_double_the_royal_paladin_count_and_drive_once() {
    for number in ["CSD03a-004", "CSD03a-SL04", "PR-287", "PR-289"] {
        let mut initial = position("CSD03a-003");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":"CSD03a-003"}));
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8,"damage":0_u8}}]);
        let mut game = drive::drive_game(&initial, &Value::Null);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("real evolution trigger")
        };
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap()
        else {
            panic!("formal opposing follower target")
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("formal target")
        };
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            4_u8,
            "the evolved holder and its ally are two Royal Paladin followers"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["evolved_with"],
            "form"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9_u8,
            "the real CSD03a-003 evolution ability pays PP 1"
        );
        game.resume_internal_card().unwrap();
        single(&mut game, number);
    }
}
