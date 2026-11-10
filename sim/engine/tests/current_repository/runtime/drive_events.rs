#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected prompts invalidate the printed Drive observer scenarios."
)]
use super::*;

fn position(observer: &str) -> Value {
    let mut initial = drive::position("CP03-025", "CP03-026", 5);
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"observer","card":observer}));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    initial
}

fn checks(board: &mut Game, accept: bool) -> Activation {
    let mut play = drive::attack(board, "CP03-026");
    while let CardPlay::Awaiting(q) = play {
        let InputDomain::DriveTrigger { .. } = q.domain else {
            panic!("Drive question")
        };
        play = board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::DriveTrigger {
                    accept,
                    target: None,
                },
            )
            .unwrap();
    }
    resolved(play)
}

fn pending(board: &mut Game) -> usize {
    let mut count = 0_usize;
    loop {
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(q) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending ability")
        };
        count = count.checked_add(1).unwrap();
        let mut play = board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
                _ => panic!("unexpected observer prompt"),
            };
            play = board
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        if matches!(play, CardPlay::Resolved(_)) {
            resolved(play);
        }
    }
    count
}

fn facts() -> Value {
    json!({"BP01-037":{"verified":true,"trigger_icon":"heal"},"BP01-006":{"verified":true,"trigger_icon":"heal"}})
}

#[test]
fn checked_observers_use_the_drive_holder_and_wait_for_both_checks_even_when_declined() {
    for (number, checks_effect) in [
        ("CP03-039", 0_u8),
        ("CP03-P27", 0),
        ("CP03-065", 1),
        ("CP03-SL14", 1),
        ("CP03-094", 2),
        ("CP03-P63", 2),
    ] {
        let mut board = drive::drive_game(&position(number), &facts());
        let result = checks(&mut board, false);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|f| matches!(f, Fact::DriveChecked(_)))
                .count(),
            2_usize
        );
        let before = board.projection(View::Referee).unwrap();
        assert_eq!(
            before["objects"]["source"]["power"], 5_u8,
            "observer effects wait for the completed Twin Drive"
        );
        assert_eq!(
            pending(&mut board),
            2_usize,
            "each completed check has one occurrence, including rejected triggers"
        );
        let view = board.projection(View::Referee).unwrap();
        match checks_effect {
            0 => {
                assert_eq!(view["objects"]["source"]["power"], 7_u8);
                assert_eq!(view["objects"]["observer"]["power"], 2_u8);
            }
            1 => assert_eq!(view["P2"]["leader"]["life"], 18_u8),
            2 => assert_eq!(view["P1"]["cemetery"], json!(["third", "tail"])),
            _ => panic!("unknown observer case"),
        }
    }
}

#[test]
fn trigger_observers_require_performed_effects_and_per_turn_limits_admit_only_one() {
    for (number, effect) in [
        ("CP03-110", 0_u8),
        ("CP03-P74", 0),
        ("CP03-113", 1),
        ("CP03-P77", 1),
        ("CP03-118", 2),
        ("CP03-P81", 2),
        ("CP03-121", 3),
        ("CP03-P84", 3),
    ] {
        for accept in [false, true] {
            let mut board = drive::drive_game(&position(number), &facts());
            checks(&mut board, accept);
            let count = pending(&mut board);
            assert_eq!(
                count,
                if !accept {
                    0
                } else if effect < 2 {
                    1
                } else {
                    2
                }
            );
            let view = board.projection(View::Referee).unwrap();
            match effect {
                0 => assert_eq!(
                    view["P1"]["hand"],
                    if accept { json!(["third"]) } else { json!([]) }
                ),
                1 => {
                    assert_eq!(
                        view["P2"]["leader"]["life"],
                        if accept { 18_u8 } else { 20 }
                    );
                    assert_eq!(
                        view["P1"]["leader"]["life"],
                        if accept { 18_u8 } else { 10 }
                    );
                }
                2 => assert_eq!(
                    view["objects"]["observer"]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("疾走")),
                    accept
                ),
                3 => assert_eq!(
                    view["P1"]["leader"]["life"],
                    if accept { 18_u8 } else { 10 }
                ),
                _ => panic!("unknown observer case"),
            }
        }
    }
}

#[test]
fn cocoa_requires_a_target_before_healing_and_amaterasu_resolves_each_mode() {
    for number in ["CP03-112", "CP03-P76", "PR-283", "CP03-107", "CP03-SL23"] {
        let mut board = drive::drive_game(&position(number), &facts());
        checks(&mut board, true);
        pending(&mut board);
        let view = board.projection(View::Referee).unwrap();
        if number == "CP03-107" || number == "CP03-SL23" {
            assert_eq!(view["P2"]["banish"], json!(["enemy"]));
            assert_eq!(
                view["P1"]["hand"],
                json!(["third"]),
                "the second occurrence has no banish target and draws (Q1587)"
            );
        } else {
            assert_eq!(view["objects"]["enemy"]["damage"], 8_u8);
            assert_eq!(view["P1"]["leader"]["life"], 20_u8);
        }
    }
    let mut initial = position("CP03-112");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut board = drive::drive_game(&initial, &facts());
    checks(&mut board, true);
    assert_eq!(
        pending(&mut board),
        2_usize,
        "unplayable target abilities are discarded by the common play procedure"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        16_u8,
        "no target means no observer healing (Q1439)"
    );
}

#[test]
fn no_icon_checks_and_foreign_field_observers_do_not_create_trigger_occurrences() {
    let mut initial = position("CP03-121");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"foreign","card":"CP03-039"},{"id":"foreign-trigger","card":"CP03-121"}]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"checked-observer","card":"CP03-039"}));
    let verified = json!({"BP01-037":{"verified":true,"trigger_icon":"none"},"BP01-006":{"verified":true,"trigger_icon":"none"}});
    let mut board = drive::drive_game(&initial, &verified);
    checks(&mut board, false);
    assert_eq!(
        pending(&mut board),
        2_usize,
        "no-icon checks still trigger the own checked observer"
    );
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["power"], 7_u8);
    assert_eq!(view["objects"]["foreign"]["power"], 2_u8);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_u8
    );
}

#[test]
fn empty_drive_deck_produces_no_completed_check_for_observers() {
    let mut initial = position("CP03-039");
    initial["players"]["P1"]["zones"]["deck"] = json!([]);
    let mut board = drive::drive_game(&initial, &Value::Null);
    let result = checks(&mut board, false);
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::DriveChecked(_)))
    );
    assert_eq!(pending(&mut board), 0_usize);
}
