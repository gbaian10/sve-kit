#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate these printed mode scenarios."
)]
use super::*;

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("DSD01a-008" | "BP20-037" | "BP15-PR12" | "BP20-P62" | "BP20-P63" | "CP02-T01" | "BP03-T02")));
        // Inert companions isolate the printed mode predicates.
        for (number, traits) in [("mode-idol", vec!["アイドル"]), ("mode-academy", vec!["学院"]), ("mode-other", vec![]), ("mode-enemy", vec![])] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":"1","power":"2","hp":"20","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "mode-targets",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["mode-idol", "mode-academy", "mode-other", "mode-enemy"] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}

fn token_name(game: &Game, zone: &str, name: &str) {
    let state = game.projection(View::Referee).unwrap();
    let tokens: Vec<_> = state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| !matches!(id.as_str(), Some("source" | "offering")))
        .collect();
    assert_eq!(tokens.len(), 1, "exactly the printed token is created");
    let number = state["objects"][tokens[0].as_str().unwrap()]["card"]
        .as_str()
        .unwrap();
    let (snapshot, _, _) = inputs();
    let row = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["number"] == number)
        .unwrap();
    assert_eq!(
        row["faces"][0]["name"], name,
        "the token uses the authoritative printing"
    );
}

fn declare(game: &mut Game, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let option = options.iter().find(|option| option.line == line).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn finish_bp20(game: &mut Game, mut play: CardPlay, mode: u64, offering: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Modes { modes, .. } => {
                assert_eq!(
                    modes,
                    if offering {
                        &[ModeId(1), ModeId(2)][..]
                    } else {
                        &[ModeId(2)][..]
                    },
                    "Q2648 permits mode one only when another own idol is available"
                );
                if !offering {
                    let before = game.projection(View::Referee).unwrap();
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Modes(vec![ModeId(1)]),
                    )
                    .unwrap_err();
                    assert_eq!(
                        game.projection(View::Referee).unwrap(),
                        before,
                        "Q2648 rejects mode 1 when only the source is an idol"
                    );
                }
                Answer::Modes(vec![ModeId(mode)])
            }
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the other own idol is a target"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            _ => panic!("unexpected printed mode question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
}

#[test]
fn bp20_037_selects_its_complete_token_modes_and_requires_another_idol_for_the_first() {
    for (mode, offering) in [(1_u64, true), (2, true), (2, false)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP20-037"}]);
        if offering {
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"offering","card":"mode-idol"}]);
        }
        let mut game = game(&initial);
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the printed follower enters before its modes"
        );
        let play = declare(&mut game, 1);
        finish_bp20(&mut game, play, mode, offering);
        let state = game.projection(View::Referee).unwrap();
        if mode == 1 {
            token_name(&game, "ex", "奏絶の独唱");
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["offering"]),
                "the selected other idol is destroyed"
            );
        } else {
            token_name(&game, "field", "新約・白の章");
            assert_eq!(
                state["P1"]["ex"],
                json!([]),
                "the other mode does not create a Solo"
            );
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "The printed modes share one table of recipients, payments, and graveyard boundaries."
)]
#[test]
fn dsd01a_008_declares_either_recipient_before_the_selected_modes_payment_and_draw() {
    for (number, mode, grave, leader, enemy) in [
        ("DSD01a-008", 1_u64, 0_u8, true, true),
        ("DSD01a-P02", 1, 0, false, true),
        ("DSD01a-008", 2, 9, true, false),
        ("DSD01a-P02", 2, 10, false, true),
        ("DSD01a-008", 2, 10, true, false),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"mode-enemy"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..grave)
                .map(|i| json!({"id":format!("academy-{i}"),"card":"mode-academy"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw1","card":"mode-other"},{"id":"draw2","card":"mode-other"},{"id":"draw3","card":"mode-other"}]);
        if enemy {
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"mode-enemy"}]);
        }
        let mut game = game(&initial);
        let CardPlay::Awaiting(question) = game
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("mode before target")
        };
        let InputDomain::Modes { modes, .. } = &question.domain else {
            panic!("modes")
        };
        assert_eq!(
            modes,
            &[ModeId(1), ModeId(2)],
            "the leader keeps both modes feasible even on an empty opposing field"
        );
        let mut mode_play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(mode)]),
            )
            .unwrap();
        if mode == 2 {
            let CardPlay::Awaiting(fee) = mode_play else {
                panic!("mode fee")
            };
            assert_eq!(fee.domain, InputDomain::OptionalCost);
            mode_play = game
                .answer_internal_card(fee.actor, fee.token, Answer::OptionalCost(true))
                .unwrap();
        }
        let CardPlay::Awaiting(target_question) = mode_play else {
            panic!("selected mode target")
        };
        let InputDomain::Targets(domain) = &target_question.domain else {
            panic!("recipient targets")
        };
        assert_eq!(domain.members.len(), if enemy { 2 } else { 1 });
        assert!(domain.members.iter().all(|m| m.player.seat == 2));
        let handle = domain
            .members
            .iter()
            .find(|m| m.card.is_none() == leader)
            .unwrap()
            .handle;
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["pp"]["current"], 10_u8,
            "all formal declarations precede base and selected-mode fees"
        );
        for handles in [vec![], vec![handle, handle], vec![999]] {
            game.answer_internal_card(
                target_question.actor,
                target_question.token,
                Answer::Targets(handles),
            )
            .unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "invalid recipient answers are atomic"
            );
        }
        resolved(
            game.answer_internal_card(
                target_question.actor,
                target_question.token,
                Answer::Targets(vec![handle]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        let damage = if mode == 1 {
            4_u8
        } else if grave == 10 {
            8
        } else {
            0
        };
        assert_eq!(
            state["P1"]["pp"]["current"],
            if mode == 1 { 5_u8 } else { 2 }
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            20_u8 - if leader { damage } else { 0 }
        );
        if enemy {
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if leader { 0_u8 } else { damage }
            );
        }
        assert_eq!(
            state["P1"]["hand"].as_array().unwrap().len(),
            if mode == 1 {
                1
            } else if grave == 10 {
                2
            } else {
                0
            },
            "the official 2024-04-26 clarification puts both damage and draw under the ten-card condition"
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("spell"))
        );
    }
}

#[test]
fn dsd01a_008_can_choose_the_paid_mode_and_decline_its_fee_with_only_seven_pp() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"DSD01a-008"}]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(question) = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("modes")
    };
    let InputDomain::Modes { modes, .. } = &question.domain else {
        panic!("mode domain")
    };
    assert_eq!(
        modes,
        &[ModeId(1), ModeId(2)],
        "CR10.4.7 and Q551/Q699 allow declining the mode fee"
    );
    let CardPlay::Awaiting(fee) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(2)]),
        )
        .unwrap()
    else {
        panic!("optional fee")
    };
    assert_eq!(fee.domain, InputDomain::OptionalCost);
    let CardPlay::Awaiting(target) = game
        .answer_internal_card(fee.actor, fee.token, Answer::OptionalCost(false))
        .unwrap()
    else {
        panic!("formal target remains required")
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("recipient domain")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card, None);
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![]))
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "declining the fee does not waive a formal target"
    );
    resolved(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 2_u8,
        "only the intrinsic five PP is paid"
    );
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(
        state["P1"]["hand"],
        json!([]),
        "declining skips both damage and draw"
    );
    assert_eq!(state["P1"]["cemetery"], json!(["spell"]));
}
