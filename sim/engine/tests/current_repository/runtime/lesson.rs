use super::*;
use sve_engine::game::internal::DecisionView;

pub(super) fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "lesson",
    )
    .unwrap()
}

pub(super) fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"first","card":"PR-155"},
        {"id":"second","card":"PR-156"},
        {"id":"wrong","card":"BP07-T02"}
    ]);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"opponent","card":"PR-155"}]);
    initial
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "An unexpected Lesson question invalidates the scenario."
)]
fn finish_choices(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            _ => panic!("unexpected Lesson question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

pub(super) fn play_with_lesson(number: &str, activated: bool, hand: Value) -> (Game, Value) {
    let mut initial = position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"item0","card":"PR-155"},{"id":"item1","card":"PR-155"},
        {"id":"item2","card":"PR-155"},{"id":"item3","card":"PR-155"},
        {"id":"item4","card":"PR-155"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"power":3_u8,"hp":10_u8,"max_hp":10_u8}},
        {"id":"other","card":"BP01-037","state":{"power":3_u8,"hp":10_u8,"max_hp":10_u8}}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = hand;
    if activated {
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    } else {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"id":"source","card":number}));
    }
    let mut game = game(&initial);
    let before;
    let play = if activated {
        before = game.projection(View::Referee).unwrap();
        let id = selection().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| {
                ability.node.kind == AbilityKind::Activated
                    && matches!(ability.node.source.origin, AbilityOrigin::Printed { .. })
            })
            .unwrap()
            .node
            .id;
        game.activate_internal_card("source", id, PlayerId { seat: 1 })
            .unwrap()
    } else {
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(
            matches!(finish_choices(&mut game, play), CardPlay::Entered(_)),
            "the real source enters before its Fanfare"
        );
        before = game.projection(View::Referee).unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("real Fanfare")
        };
        let option = options.iter().find(|option| option.card == number).unwrap();
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
            .unwrap()
    };
    resolved(finish_choices(&mut game, play));
    (game, before)
}

fn materials(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("Lesson material choice")
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("Lesson materials")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the controller's named EX cards qualify"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(1),
        "Lesson 1 requires one item"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "Lesson 1 cannot consume extra items"
    );
    question
}

fn accept_head(game: &mut Game, play: CardPlay) -> CardPlay {
    if let CardPlay::Awaiting(question) = &play
        && question.domain == InputDomain::OptionalCost
    {
        game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
            .unwrap()
    } else {
        play
    }
}

#[test]
fn cp02_003_and_printings_banish_lesson_material_after_pp_and_gain_storm() {
    for number in ["CP02-003", "CP02-SL03", "PR-152"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        let mut game = game(&initial);
        let id = selection().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let question = materials(
            game.activate_internal_card("source", id, PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10_u8,
            "all parameters are chosen before ordered payment commits"
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(state["P1"]["ex"], json!(["second", "wrong"]));
        assert_eq!(state["P2"]["ex"], json!(["opponent"]));
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
}

#[test]
fn cp02_023_and_printing_pay_lesson_on_fanfare_before_healing() {
    for number in ["CP02-023", "CP02-P17"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("Fanfare")
        };
        let play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let play = accept_head(&mut game, play);
        let question = materials(play);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20_u8
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![1]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 21_u8);
        assert_eq!(state["P1"]["ex"], json!(["first", "wrong"]));
        assert_eq!(
            state["P1"]["cemetery"],
            json!([]),
            "Lesson banishes rather than discards"
        );
    }
}

#[test]
fn lesson_cannot_be_paid_from_hand_field_opponent_ex_or_wrong_named_ex() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"wrong","card":"BP07-T02"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"hand-item","card":"PR-155"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP02-003"},{"id":"field-item","card":"PR-155"}
    ]);
    let mut game = game(&initial);
    let id = selection().current["CP02-003"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", id, PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "failed payment is atomic"
    );
}

#[test]
fn cp02_040_and_printing_require_two_lesson_cards_after_real_evolution() {
    for (base, form) in [("CP02-039", "CP02-040"), ("CP02-P30", "CP02-P31")] {
        for enough in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"hp":7_u8,"max_hp":7_u8,"damage":0_u8}}]);
            if !enough {
                initial["players"]["P1"]["zones"]["ex"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            }
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("Evolution trigger")
            };
            let play = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            let play = accept_head(&mut game, play);
            let play = if let CardPlay::Awaiting(target) = play {
                assert!(matches!(target.domain, InputDomain::Targets(_)));
                game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
                    .unwrap()
            } else {
                play
            };
            if enough {
                let CardPlay::Awaiting(cost) = play else {
                    panic!("Two Lesson materials")
                };
                let InputDomain::CostMaterials(domain) = &cost.domain else {
                    panic!("Cost materials")
                };
                assert_eq!(domain.cardinality.minimum, Nat(2));
                assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
                let before = game.projection(View::Referee).unwrap();
                game.answer_internal_card(cost.actor, cost.token, Answer::CostMaterials(vec![0]))
                    .unwrap_err();
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    before,
                    "partial Lesson payment is rejected atomically"
                );
                resolved(
                    game.answer_internal_card(
                        cost.actor,
                        cost.token,
                        Answer::CostMaterials(vec![0, 1]),
                    )
                    .unwrap(),
                );
            } else {
                assert!(
                    matches!(play, CardPlay::Cancelled),
                    "an unpayable evolution trigger never deals damage"
                );
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if enough { 4_u8 } else { 0 }
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 9_u8,
                "evolution payment remains separate from Lesson"
            );
            assert_eq!(
                state["P1"]["ex"],
                if enough {
                    json!(["wrong"])
                } else {
                    json!(["first", "wrong"])
                }
            );
        }
    }
}
