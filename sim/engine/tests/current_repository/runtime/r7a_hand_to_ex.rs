#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed hand-to-EX payment scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn trigger(board: &mut Game, number: &str) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed payment or resolution choice")
    };
    question
}

fn accept(board: &mut Game, play: CardPlay) -> CardPlay {
    if let CardPlay::Awaiting(question) = &play
        && question.domain == InputDomain::OptionalCost
    {
        board
            .answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
            .unwrap()
    } else {
        play
    }
}

#[test]
fn r7a_hand_to_ex_coral_printings_lock_the_enemy_before_private_ocean_payment() {
    for form in ["BP22-065", "BP22-P35"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-064"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"material","card":"BP22-064"},{"id":"wrong","card":"BP01-034"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP14-008"}]);
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the real base produces the printed evolution trigger"
        );
        let declaration = trigger(&mut board, form);
        let accepted = accept(&mut board, declaration);
        let target = question(accepted);
        let InputDomain::Targets(targets) = &target.domain else {
            panic!("formal enemy target before material")
        };
        assert_eq!(
            targets.members.len(),
            1,
            "the sole opposing follower is the formal target"
        );
        let payment = board
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![targets.members[0].handle]),
            )
            .unwrap();
        let material = question(payment);
        let InputDomain::CostMaterials(materials) = &material.domain else {
            panic!("private Ocean material")
        };
        assert_eq!(
            materials.members.len(),
            1,
            "only the Ocean hand card qualifies"
        );
        assert!(
            board.internal_decision(View::P2).is_none(),
            "the opponent cannot inspect unspent hand materials"
        );
        resolved(
            board
                .answer_internal_card(
                    material.actor,
                    material.token,
                    Answer::CostMaterials(vec![materials.members[0].handle]),
                )
                .unwrap(),
        );
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["ex"],
            json!(["material"]),
            "the actual payment reaches EX"
        );
        assert_eq!(
            after["P1"]["hand"],
            json!(["wrong"]),
            "other traits remain in hand"
        );
        assert_eq!(
            after["P1"]["leader"]["life"], 11_u8,
            "the complete printed body heals after payment"
        );
        assert_eq!(
            after["P2"]["cemetery"],
            json!(["enemy"]),
            "four damage destroys the two-life follower"
        );
    }
}

#[test]
fn r7a_hand_to_ex_coral_without_an_enemy_does_not_pay_or_heal_q2840() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-064"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":"BP22-064"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-065"}]);
    let mut board = game(&initial);
    board
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    let play = trigger(&mut board, "BP22-065");
    let play = accept(&mut board, play);
    assert!(
        matches!(play, CardPlay::Cancelled),
        "a mandatory formal target is unavailable"
    );
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["hand"],
        json!(["material"]),
        "no target means no private hand payment"
    );
    assert_eq!(
        after["P1"]["leader"]["life"], 10_u8,
        "Q2840 forbids the isolated heal"
    );
}

fn anastasia_position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":number},{"id":"picked","card":"ECP02-001"},
        {"id":"wrong","card":"BP01-034"},{"id":"expensive","card":"CP02-013"},
        {"id":"spell","card":"CP02-011"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"ECP02-002"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"lesson","card":"PR-155"},{"id":"e1","card":"BP01-034"},
        {"id":"e2","card":"BP01-034"},{"id":"e3","card":"BP01-034"},{"id":"e4","card":"BP01-034"}
    ]);
    initial
}

fn assert_discount_scope(board: &mut Game) {
    let CardPlay::Awaiting(ward) = board
        .play_internal_card("expensive", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the unplaced Ward follower offers entry posture")
    };
    assert!(
        matches!(
            board
                .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the unplaced Cinderella follower enters normally"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3_u8,
        "the discount applies only to the card that actually reached EX"
    );
    finish_checks(board);
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "the vanilla Ward entry leaves no pending ability"
    );
    assert!(
        matches!(
            board
                .play_internal_card("picked", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the placed follower can be played from EX"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3_u8,
        "the placed cost-two follower is discounted by two"
    );
}

#[test]
fn r7a_hand_to_ex_anastasia_lesson_frees_full_ex_and_discounts_only_the_placed_card() {
    for number in ["ECP02-001", "ECP02-SL01"] {
        for place in [false, true] {
            let mut board = game(&anastasia_position(number));
            assert!(
                matches!(
                    board
                        .play_internal_card("source", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "the source enters before its paid Fanfare"
            );
            let declaration = trigger(&mut board, number);
            let accepted = accept(&mut board, declaration);
            let lesson = question(accepted);
            let InputDomain::CostMaterials(materials) = &lesson.domain else {
                panic!("Lesson payment")
            };
            assert_eq!(
                materials.members.len(),
                1,
                "only the magic item can pay Lesson 1"
            );
            let choosing = board
                .answer_internal_card(
                    lesson.actor,
                    lesson.token,
                    Answer::CostMaterials(vec![materials.members[0].handle]),
                )
                .unwrap();
            let choice = question(choosing);
            let InputDomain::Targets(candidates) = &choice.domain else {
                panic!("optional private hand selection")
            };
            assert_eq!(
                candidates.members.len(),
                1,
                "only the cost-two Cinderella follower qualifies; spells and costlier cards do not"
            );
            assert!(
                board.internal_decision(View::P2).is_none(),
                "unplaced hand cards stay private"
            );
            let selected = if place {
                vec![candidates.members[0].handle]
            } else {
                vec![]
            };
            resolved(
                board
                    .answer_internal_card(choice.actor, choice.token, Answer::Targets(selected))
                    .unwrap(),
            );
            finish_checks(&mut board);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(
                after["P1"]["pp"]["current"], 7_u8,
                "the base and ordered Fanfare cost two plus one PP"
            );
            assert!(
                after["objects"].get("lesson").is_none(),
                "the magic-item token is erased after Lesson banishes it"
            );
            assert_eq!(
                after["P1"]["ex"],
                if place {
                    json!(["e1", "e2", "e3", "e4", "picked"])
                } else {
                    json!(["e1", "e2", "e3", "e4"])
                },
                "Q2151 frees one slot before only the optional arrival refills it"
            );
            if place {
                assert_discount_scope(&mut board);
            } else {
                assert!(
                    after["P1"]["hand"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("picked")),
                    "declining leaves the eligible card in hand"
                );
                let action =
                    json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}});
                assert_eq!(
                    board.internal_rule_action(&action).unwrap().outcome,
                    "resolved",
                    "the complete card retains its one-PP evolution"
                );
            }
        }
    }
}
