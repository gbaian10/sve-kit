#![expect(
    clippy::panic,
    reason = "Unexpected real-card input domains fail this serial scenario."
)]

use super::*;
use sve_engine::game::player::{
    AttackTarget, Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, Question,
    QuestionShape, SearchAnswer, SubmissionKind, Viewer,
};
use sve_engine::ir::ModeId;

fn question(game: &Game, actor: Viewer) -> Question {
    let Input::Question(question) = game.input(actor) else {
        panic!("player question");
    };
    *question
}

fn main_intents(initial: &Value) -> Vec<MainIntentKind> {
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "main-payment-availability",
    )
    .unwrap();
    game.advance_player_input();
    let QuestionShape::ActionWindow(actions) = question(&game, Viewer::P1).shape else {
        panic!("main actions");
    };
    actions.intents.into_iter().map(|intent| intent.kind).collect()
}

#[test]
fn player_main_actions_hide_play_without_a_payable_exchange_route() {
    for (field, ex, pp, available) in [
        (2_u8, 2_u8, 0_u8, false),
        (4, 0, 0, true),
        (0, 4, 0, true),
        (2, 2, 9, true),
    ] {
        let mut initial = draw_position();
        let p1 = &mut initial["players"]["P1"];
        p1["pp"]["current"] = json!(pp);
        p1["zones"]["hand"] = json!([{"id":"source","card":"BP01-103"}]);
        p1["zones"]["field"] = json!(
            (0..field)
                .map(|n| json!({"id":format!("field{n}"),"card":"BP04-T01"}))
                .collect::<Vec<_>>()
        );
        p1["zones"]["ex"] = json!(
            (0..ex)
                .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-125"}))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            main_intents(&initial).iter().any(|intent| matches!(
                intent, MainIntentKind::Play { card } if card.properties.card == "BP01-103"
            )),
            available,
            "field {field}, EX {ex}, PP {pp}",
        );
    }
}

#[test]
fn player_main_actions_require_a_target_with_an_affordable_price() {
    let mut initial = draw_position();
    let p1 = &mut initial["players"]["P1"];
    p1["zones"]["hand"] = json!([{"id":"source","card":"BP17-030"}]);
    p1["zones"]["ex"] = json!(
        (0_u8..5)
            .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    for (follower, pp, available) in [
        ("BP17-035", 0_u8, false),
        ("BP17-022", 0, true),
        ("BP17-035", 2, true),
    ] {
        initial["players"]["P1"]["pp"]["current"] = json!(pp);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"assassin","card":follower}]);
        assert_eq!(
            main_intents(&initial).iter().any(|intent| matches!(
                intent, MainIntentKind::Play { card } if card.properties.card == "BP17-030"
            )),
            available,
            "{follower}, PP {pp}, full EX",
        );
    }
}

#[test]
fn player_main_actions_require_fixed_pp_even_when_a_mode_is_pending() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-030"}]);
    for (princess, pp, available) in [(false, 0_u8, false), (true, 0, true), (false, 1, true)] {
        let p1 = &mut initial["players"]["P1"];
        p1["pp"]["current"] = json!(pp);
        p1["zones"]["field"] = if princess {
            json!([{"id":"princess","card":"BP01-026"}])
        } else {
            json!([])
        };
        assert_eq!(
            main_intents(&initial).iter().any(|intent| matches!(
                intent, MainIntentKind::Play { card } if card.properties.card == "BP03-030"
            )),
            available,
            "Princess {princess}, PP {pp}",
        );
    }
}

#[test]
fn player_main_actions_require_ex_space_for_a_mandatory_hand_transfer() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let p1 = &mut initial["players"]["P1"];
    p1["zones"]["hand"] = json!([
        {"id":"wolf","card":"BP10-017"},
        {"id":"material","card":"BP17-071"}
    ]);
    p1["zones"]["field"] = json!([{"id":"archer","card":"BP17-071"}]);
    for ex in [4_u8, 5] {
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0..ex)
                .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let intents = main_intents(&initial);
        for printing in ["BP10-017", "BP17-071"] {
            assert_eq!(
                intents.iter().any(|intent| matches!(
                    intent, MainIntentKind::Activate { holder, .. } if holder.properties.card == printing
                )),
                ex == 4,
                "{printing}, EX {ex}",
            );
        }
    }
}

#[test]
fn bp13_002_plays_from_ex_for_three_pp_and_resolves_its_fanfare() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":3_u8});
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"secca","card":"BP13-002"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"resolve","card":"BP13-008"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "main-advance-from-ex",
    )
    .unwrap();
    game.advance_player_input();
    let main = question(&game, Viewer::P1);
    let QuestionShape::ActionWindow(actions) = main.shape else {
        panic!("main actions");
    };
    let intent = actions
        .intents
        .into_iter()
        .find(|intent| {
            matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == "BP13-002")
        })
        .expect("the Advance follower in EX can be played");
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::MainAction {
                token: main.token,
                intent: intent.id,
                answer: MainActionAnswer::None,
            },
        )
        .kind,
        SubmissionKind::Applied
    );
    let answer = |board: &mut Game, prompt: Question, response| {
        assert_eq!(
            board.submit_player(
                Viewer::P1,
                Command::Answer {
                    token: prompt.token,
                    answer: response,
                },
            )
            .kind,
            SubmissionKind::Applied
        );
    };
    let pending = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Pending(options)) = &pending.shape else {
        panic!("Secca's Fanfare");
    };
    assert_eq!(options.len(), 1);
    let occurrence = options[0].handle;
    answer(&mut game, pending, Answer::Pending(occurrence));
    let choice = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Modes { modes, .. }) = &choice.shape else {
        panic!("Fanfare modes");
    };
    assert_eq!(modes, &[ModeId(1)]);
    answer(&mut game, choice, Answer::Modes(vec![ModeId(1)]));
    let search = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Search(found)) = &search.shape else {
        panic!("search for 九尾の決意");
    };
    let card = found.cards[0].handle;
    answer(&mut game, search, Answer::Search(SearchAnswer::Ungrouped(vec![card])));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["field"], json!(["secca"]));
    assert_eq!(state["P1"]["hand"], json!(["resolve"]));
}

#[test]
fn player_main_actions_require_a_standing_source_for_death_declaration() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171"}]);
    for acted in [false, true] {
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"amulet","card":"BP01-150","state":{"acted":acted}}]);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "main-self-act-cost",
        )
        .unwrap();
        game.advance_player_input();
        let QuestionShape::ActionWindow(actions) = question(&game, Viewer::P1).shape else {
            panic!("main actions");
        };
        assert_eq!(
            actions.intents.iter().any(|intent| matches!(
                &intent.kind,
                MainIntentKind::Activate { holder, .. } if holder.properties.card == "BP01-150"
            )),
            !acted,
        );
    }
}

#[test]
fn player_main_actions_require_enough_spell_targets() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP22-018"}]);
    for has_target in [false, true] {
        if has_target {
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-037"}]);
        }
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "main-spell-target",
        )
        .unwrap();
        game.advance_player_input();
        let QuestionShape::ActionWindow(actions) = question(&game, Viewer::P1).shape else {
            panic!("main actions");
        };
        assert_eq!(
            actions.intents.iter().any(|intent| matches!(
                &intent.kind,
                MainIntentKind::Play { card } if card.properties.card == "BP22-018"
            )),
            has_target,
        );
    }
}

#[test]
fn player_main_actions_require_a_mode_with_legal_targets() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP13-008"}]);
    // This Sekka qualifies for mode 2 but lacks the exact name needed for mode 1.
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"sekka","card":"BP13-001"}]);
    for has_target in [false, true] {
        if has_target {
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
        }
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "main-mode-targets",
        )
        .unwrap();
        game.advance_player_input();
        let QuestionShape::ActionWindow(actions) = question(&game, Viewer::P1).shape else {
            panic!("main actions");
        };
        assert_eq!(
            actions.intents.iter().any(|intent| matches!(
                &intent.kind,
                MainIntentKind::Play { card } if card.properties.card == "BP13-008"
            )),
            has_target,
        );
    }
}

#[test]
fn player_main_actions_require_payable_activation_materials() {
    let mut initial = draw_position();
    let mut field = vec![
        json!({"id":"priest","card":"BP22-102"}),
        json!({"id":"first-amulet","card":"BP01-016"}),
        json!({"id":"second-amulet","card":"BP01-016"}),
    ];
    for can_pay in [false, true] {
        if can_pay {
            field.push(json!({"id":"third-amulet","card":"BP01-016"}));
        }
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "main-activation-materials",
        )
        .unwrap();
        game.advance_player_input();
        let QuestionShape::ActionWindow(actions) = question(&game, Viewer::P1).shape else {
            panic!("main actions");
        };
        assert_eq!(
            actions.intents.iter().any(|intent| matches!(
                &intent.kind,
                MainIntentKind::Activate { holder, line: 2, .. }
                    if holder.properties.card == "BP22-102"
            )),
            can_pay,
        );
    }
}

#[test]
fn player_main_actions_require_enough_stack_for_great_magician() {
    for (stack, can_pay) in [(2_u8, false), (3, true)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"magician","card":"BP22-044"},
            {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":stack}}}
        ]);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "main-earth-rite",
        )
        .unwrap();
        game.advance_player_input();
        let QuestionShape::ActionWindow(actions) = question(&game, Viewer::P1).shape else {
            panic!("main actions");
        };
        assert_eq!(
            actions.intents.iter().any(|intent| matches!(
                &intent.kind,
                MainIntentKind::Activate { holder, line: 2, .. }
                    if holder.properties.card == "BP22-044"
            )),
            can_pay,
        );
    }
}

#[test]
fn player_main_actions_require_a_target_within_affordable_x() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"strategist","card":"BP21-030"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"expensive","card":"BP21-055"},
        {"id":"same-name","card":"BP21-030"}
    ]);
    for (pp, can_pay) in [(1_u8, false), (2, true)] {
        initial["players"]["P1"]["pp"]["current"] = json!(pp);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "main-x-target-limit",
        )
        .unwrap();
        game.advance_player_input();
        let QuestionShape::ActionWindow(actions) = question(&game, Viewer::P1).shape else {
            panic!("main actions");
        };
        assert!(actions.intents.iter().any(|intent| matches!(intent.kind, MainIntentKind::EndPhase)));
        assert_eq!(
            actions.intents.iter().any(|intent| matches!(
                &intent.kind,
                MainIntentKind::Activate { holder, line: 1, .. }
                    if holder.properties.card == "BP21-030"
            )),
            can_pay,
        );
        let source = game.internal_activated_sources("strategist").unwrap().remove(0);
        let play = game.activate_internal_card("strategist", source.definition, PlayerId { seat: 1 }).unwrap();
        let CardPlay::Awaiting(x) = play else {
            panic!("X question");
        };
        let after_x = game.answer_internal_card(x.actor, x.token, Answer::Integer(Nat(u64::from(pp))));
        if can_pay {
            let CardPlay::Awaiting(target) = after_x.unwrap() else {
                panic!("X target question");
            };
            let InputDomain::Targets(domain) = &target.domain else {
                panic!("X target domain");
            };
            let candidate = domain.members.iter().find(|member| member.card.as_deref() == Some("BP21-055")).unwrap();
            resolved(game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![candidate.handle])).unwrap());
        } else {
            assert!(matches!(after_x, Err(EngineFailure::IllegalPlay(_))));
        }
    }
}

fn quick_pass(game: &mut Game, actor: Viewer) {
    let quick = question(game, actor);
    assert_eq!(
        game.submit_player(
            actor,
            Command::Answer {
                token: quick.token,
                answer: Answer::Quick(None)
            }
        )
        .kind,
        SubmissionKind::Applied,
        "The current Quick Pass must advance the original procedure"
    );
}

#[test]
fn player_main_actions_real_card_attack_and_turn_end_need_only_question_capabilities() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"private-authority-attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"next-draw","card":"BP01-037"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "main-real",
    )
    .unwrap();
    game.advance_player_input();
    let first = question(&game, Viewer::P1);
    let QuestionShape::ActionWindow(actions) = &first.shape else {
        panic!("main actions");
    };
    let attack = actions
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::Attack { .. }))
        .unwrap();
    let MainIntentKind::Attack { attacker, targets } = &attack.kind else {
        panic!("attack");
    };
    assert_eq!(attacker.properties.card, "BP01-037");
    let power = attacker
        .properties
        .state
        .as_ref()
        .unwrap()
        .stats
        .unwrap()
        .attack;
    assert!(power > 0);
    assert_eq!(targets, &[AttackTarget::Leader(Viewer::P2)]);
    let command = Command::MainAction {
        token: first.token,
        intent: attack.id.clone(),
        answer: MainActionAnswer::Attack(targets[0].clone()),
    };
    assert_eq!(
        game.submit_player(Viewer::P1, command.clone()).kind,
        SubmissionKind::Applied
    );
    quick_pass(&mut game, Viewer::P2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_i64 - power
    );
    let next = question(&game, Viewer::P1);
    let QuestionShape::ActionWindow(next_actions) = &next.shape else {
        panic!("main after attack");
    };
    assert_eq!(next_actions.intents.len(), 1);
    assert!(matches!(
        next_actions.intents[0].kind,
        MainIntentKind::EndPhase
    ));
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Rejected
    );
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::MainAction {
                token: next.token,
                intent: next_actions.intents[0].id.clone(),
                answer: MainActionAnswer::None
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    quick_pass(&mut game, Viewer::P2);
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::ActionWindow(_)
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P2");
    assert_eq!(state["P2"]["hand"], json!(["next-draw"]));
}

#[test]
fn basic_policy_real_attack_uses_observation_and_question_without_authority_reads() {
    use sve_engine::policy::{BasicPolicy, Choice};

    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"authority-attacker","card":"BP01-037"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "policy-real",
    )
    .unwrap();
    game.advance_player_input();
    let mut policy = BasicPolicy::new(Viewer::P1);
    let public = question(&game, Viewer::P1);
    let observation = game.observe(Viewer::P1).unwrap();
    let choice = policy.choose(&observation, &public.shape).unwrap();
    let Choice::MainAction { intent, answer } = &choice else {
        panic!("main choice");
    };
    let MainIntentKind::Attack { attacker, .. } = &intent.kind else {
        panic!("attack choice");
    };
    let power = attacker
        .properties
        .state
        .as_ref()
        .unwrap()
        .stats
        .unwrap()
        .attack;
    assert!(power > 0);
    assert_eq!(
        *answer,
        MainActionAnswer::Attack(AttackTarget::Leader(Viewer::P2))
    );
    let submission = game.submit_player(
        Viewer::P1,
        Command::MainAction {
            token: public.token,
            intent: intent.id.clone(),
            answer: answer.clone(),
        },
    );
    assert_eq!(submission.kind, SubmissionKind::Applied);
    policy.record_submission(&choice, submission.kind);
    let quick_question = question(&game, Viewer::P2);
    let mut opponent = BasicPolicy::new(Viewer::P2);
    let quick_choice = opponent
        .choose(&game.observe(Viewer::P2).unwrap(), &quick_question.shape)
        .unwrap();
    let Choice::Decision(quick_answer) = &quick_choice else {
        panic!("Quick answer");
    };
    assert_eq!(*quick_answer, Answer::Quick(None));
    let quick_submission = game.submit_player(
        Viewer::P2,
        Command::Answer {
            token: quick_question.token,
            answer: quick_answer.clone(),
        },
    );
    assert_eq!(quick_submission.kind, SubmissionKind::Applied);
    opponent.record_submission(&quick_choice, quick_submission.kind);
    let after_attack = question(&game, Viewer::P1);
    let end_choice = policy
        .choose(&game.observe(Viewer::P1).unwrap(), &after_attack.shape)
        .unwrap();
    assert!(
        matches!(end_choice, Choice::MainAction { intent: end_intent, answer: MainActionAnswer::None }
        if matches!(end_intent.kind, MainIntentKind::EndPhase))
    );
    assert_eq!(
        game.observe(Viewer::P1).unwrap().players[1].resources.life,
        20_i64.checked_sub(power).unwrap()
    );
}
