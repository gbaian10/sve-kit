use super::*;
use crate::player_pipeline::question;
use sve_battle_contract::{
    SourceRef,
    observation::{Phase, ProcessingStage},
};
use sve_engine::game::player::{DecisionShape, QuestionPurpose};

fn program(maximum: u64, whose: &str) -> WellFormedCardProgram {
    let mut raw = trigger_multiplicity::program_at(false, "end", whose)
        .card()
        .clone();
    for ability in &mut raw.abilities {
        if let Some(trigger) = &mut ability.node.trigger {
            trigger.limit = Some(LimitSpec {
                maximum: Nat(maximum),
                window: LimitWindow::Turn,
                scope: LimitScope::AbilityInstance,
                consume_at: ConsumptionPoint::PendingAdmission,
            });
        }
    }
    WellFormedCardProgram::check(raw).unwrap()
}

fn choices(
    projection: &QuestionProjection,
) -> Option<(&[wire::ChoiceOption], &[wire::EarlierChoice])> {
    if let wire::QuestionShape::Choose {
        options, earlier, ..
    } = &projection.question().shape
    {
        Some((options, earlier))
    } else {
        None
    }
}

#[test]
fn quota_earlier_choices_keep_the_frozen_occurrence_but_never_an_answer_code() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = position(actor, 1, false);
        let first = project(&game, actor, "quota-history-first");
        let selected = choices(&first).unwrap().0[2].clone();
        let command = first
            .answer(
                actor,
                &first.question().id,
                &WireAnswer::Choose {
                    chosen: vec![selected.id.clone()],
                },
            )
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command.clone()).kind,
            SubmissionKind::Applied
        );
        let native = question(&game, actor);
        let QuestionShape::Decision(DecisionShape::TriggerQuota(domain)) = native.shape else {
            panic!("second native admission group")
        };
        assert_eq!(domain.earlier.len(), 1);
        assert!(domain.earlier[0].occurrence.source.is_some());
        assert!(domain.earlier[0].occurrence.occurrence.is_some());
        let before = exact_authority(&game);
        let second = project(&game, actor, "quota-history-second");
        let (options, earlier) = choices(&second).unwrap();
        assert_eq!(earlier.len(), 1);
        assert_eq!(earlier[0].actor, seat(actor));
        assert_eq!(earlier[0].value, selected.value);
        assert_ne!(earlier[0].value, options[0].value);
        assert!(
            serde_json::to_value(&earlier[0])
                .unwrap()
                .get("id")
                .is_none()
        );
        assert_eq!(
            second.answer(
                actor,
                &second.question().id,
                &WireAnswer::Choose {
                    chosen: vec![selected.id],
                }
            ),
            Err(AnswerError)
        );
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(exact_authority(&game), before);
    }
}

fn position(actor: Viewer, maximum: u64, both_controllers: bool) -> Game {
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    let them = if actor == Viewer::P1 { "P2" } else { "P1" };
    let mut initial = setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["hand"] = json!([]);
    initial["players"][me]["zones"]["field"] = if both_controllers {
        json!([
            {"id":"first","card":"unit-follower"}
        ])
    } else {
        json!([
            {"id":"first","card":"unit-follower"},
            {"id":"last","card":"unit-follower"}
        ])
    };
    initial["players"][them]["zones"]["field"] = if both_controllers {
        json!([{"id":"other","card":"unit-follower"}])
    } else {
        json!([])
    };
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-trigger-quota",
    )
    .unwrap();
    game.load_internal_card(
        "unit-follower",
        &program(maximum, if both_controllers { "turn" } else { "self" }),
    )
    .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn trigger_quota_projects_a_real_admission_and_preserves_the_native_handle() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = position(actor, 1, false);
        let native = question(&game, actor);
        let QuestionShape::Decision(DecisionShape::TriggerQuota(domain)) = &native.shape else {
            panic!("native admission quota")
        };
        assert_eq!(native.purpose, QuestionPurpose::RuleChoice);
        assert_eq!(domain.candidates.len(), 3);
        let before = exact_authority(&game);
        let projection = project(&game, actor, "quota-first");
        assert_eq!(
            projection.question().purpose,
            wire::QuestionPurpose::RuleChoice
        );
        assert_eq!(projection.question().usage, wire::QuestionUse::TriggerQuota);
        assert_eq!(
            projection.question().processing.stage,
            Some(ProcessingStage::TriggerAdmission)
        );
        assert_eq!(
            projection.question().processing,
            projection.observation().processing[0]
        );
        assert!(projection.question().is_coherent());
        assert_eq!(projection.question().rules, None);
        let wire::QuestionShape::Choose {
            options,
            bounds,
            earlier,
            ..
        } = &projection.question().shape
        else {
            panic!("choose a quota occurrence")
        };
        assert_eq!(options.len(), 3);
        assert_eq!(bounds.min.get(), 1);
        assert_eq!(bounds.max.unwrap().get(), 1);
        assert_eq!(earlier.as_slice(), []);
        let answer = WireAnswer::Choose {
            chosen: vec![options[2].id.clone()],
        };
        let command = projection
            .answer(actor, &projection.question().id, &answer)
            .unwrap();
        assert_eq!(
            command,
            Command::Answer {
                token: native.token,
                answer: Answer::TriggerQuota(domain.candidates[2].handle),
            }
        );
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
    }
}

#[test]
fn quota_earlier_controller_and_holder_are_authorized_for_the_next_answering_seat() {
    for turn_actor in [Viewer::P1, Viewer::P2] {
        let mut game = position(turn_actor, 1, true);
        let first = project(&game, turn_actor, "quota-turn-first");
        let options = choices(&first).unwrap().0;
        assert_eq!(options.len(), 2);
        let command = first
            .answer(
                turn_actor,
                &first.question().id,
                &WireAnswer::Choose {
                    chosen: vec![options[1].id.clone()],
                },
            )
            .unwrap();
        assert_eq!(
            game.submit_player(turn_actor, command).kind,
            SubmissionKind::Applied
        );
        let actor = opposite(turn_actor);
        let native = question(&game, actor);
        let QuestionShape::Decision(DecisionShape::TriggerQuota(domain)) = native.shape else {
            panic!("the other controller chooses after the turn player")
        };
        assert_eq!(domain.earlier[0].actor, turn_actor);
        let native_holder = domain.earlier[0]
            .occurrence
            .source
            .as_ref()
            .unwrap()
            .holder
            .as_ref()
            .unwrap();
        let before = exact_authority(&game);
        let second = project(&game, actor, "quota-next-controller");
        assert_eq!(second.question().actor, seat(actor));
        assert_eq!(second.question().processing.controller, Some(seat(actor)));
        let (options, earlier) = choices(&second).unwrap();
        assert_eq!(earlier[0].actor, seat(turn_actor));
        assert_ne!(earlier[0].value, options[0].value);
        assert_earlier_source(
            &earlier[0].value,
            &second,
            turn_actor,
            native_holder.as_str(),
        )
        .unwrap();
        assert_eq!(exact_authority(&game), before);
    }
}

fn assert_earlier_source(
    value: &wire::ChoiceValue,
    projection: &QuestionProjection,
    turn_actor: Viewer,
    native_holder: &str,
) -> Option<()> {
    let wire::ChoiceValue::Occurrence {
        source:
            SourceRef::Card {
                printing_id,
                face: Some(face),
                line: Some(line),
                holder: Some(holder),
            },
        controller,
        event: wire::TriggerEvent::Phase { phase, player, .. },
        position,
    } = value
    else {
        return None;
    };
    assert_eq!(
        printing_id, "printing-follower",
        "the frozen printed source is retained"
    );
    assert_eq!(face.get(), 1, "the adopted face is used");
    assert_eq!(line.get(), 2, "the triggered line is retained");
    assert_eq!(
        *controller,
        seat(turn_actor),
        "the historical controller is separate from the current actor"
    );
    assert_eq!(
        *player,
        seat(turn_actor),
        "the event belongs to the turn player"
    );
    assert_eq!(
        *phase,
        Phase::End,
        "the actual end phase produced this occurrence"
    );
    assert_eq!(*position, None, "the phase event has no source slot");
    assert_eq!(
        holder.as_str(),
        native_holder,
        "the current viewer's authorized holder is used"
    );
    assert!(
        projection.observation().objects.contains_key(holder),
        "the link belongs to this object scope"
    );
    Some(())
}

#[test]
fn quota_answers_reject_bad_shapes_incomplete_duplicate_foreign_wrong_seat_and_stale_choices() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = position(actor, 1, false);
        let projection = project(&game, actor, "quota-validate");
        let id = &projection.question().id;
        let options = choices(&projection).unwrap().0;
        assert_eq!(options.len(), 3);
        let before = exact_authority(&game);
        for invalid in [
            WireAnswer::Choose { chosen: vec![] },
            WireAnswer::Choose {
                chosen: vec![options[0].id.clone(); 2],
            },
            WireAnswer::Choose {
                chosen: vec![options[0].id.clone(), options[1].id.clone()],
            },
            WireAnswer::Choose {
                chosen: vec![OptionId::new("foreign:c0".into())],
            },
            WireAnswer::Quick { option: None },
        ] {
            assert_eq!(projection.answer(actor, id, &invalid), Err(AnswerError));
        }
        let answer = WireAnswer::Choose {
            chosen: vec![options[1].id.clone()],
        };
        assert_eq!(
            projection.answer(opposite(actor), id, &answer),
            Err(AnswerError)
        );
        assert_eq!(
            projection.answer(actor, &QuestionId::new("other".into()), &answer),
            Err(AnswerError)
        );
        let command = projection.answer(actor, id, &answer).unwrap();
        assert_eq!(
            game.submit_player(opposite(actor), command.clone()).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            game.submit_player(actor, command.clone()).kind,
            SubmissionKind::Applied
        );
        let after = exact_authority(&game);
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(exact_authority(&game), after);
    }
}

fn budgets(projection: &QuestionProjection) -> Vec<u64> {
    projection
        .observation()
        .objects
        .values()
        .filter_map(|object| object.state.as_ref())
        .filter_map(|state| state.quotas.as_ref())
        .flatten()
        .map(|quota| quota.remaining.get())
        .collect()
}

#[test]
fn repeated_quota_admission_reindexes_candidates_and_commits_budgets_only_after_all_choices() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = position(actor, 2, false);
        let mut retired: Vec<OptionId> = vec![];
        for (index, expected) in [3_usize, 2, 3, 2].into_iter().enumerate() {
            let native = question(&game, actor);
            let QuestionShape::Decision(DecisionShape::TriggerQuota(domain)) = native.shape else {
                panic!("the next one-at-a-time admission")
            };
            assert_eq!(domain.candidates.len(), expected);
            let before = exact_authority(&game);
            let projection = project(&game, actor, &format!("quota-budget-{index}"));
            let (options, earlier) = choices(&projection).unwrap();
            assert_eq!(options.len(), expected);
            assert_eq!(earlier.len(), index);
            assert_eq!(budgets(&projection), [2, 2]);
            for old in &retired {
                assert_eq!(
                    projection.answer(
                        actor,
                        &projection.question().id,
                        &WireAnswer::Choose {
                            chosen: vec![old.clone()],
                        }
                    ),
                    Err(AnswerError)
                );
            }
            let chosen = options.last().unwrap();
            retired.push(chosen.id.clone());
            let command = projection
                .answer(
                    actor,
                    &projection.question().id,
                    &WireAnswer::Choose {
                        chosen: vec![chosen.id.clone()],
                    },
                )
                .unwrap();
            assert_eq!(
                command,
                Command::Answer {
                    token: native.token,
                    answer: Answer::TriggerQuota(domain.candidates.last().unwrap().handle),
                }
            );
            assert_eq!(exact_authority(&game), before);
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
        }
        let native = question(&game, actor);
        assert!(
            matches!(&native.shape, QuestionShape::Decision(DecisionShape::Pending(options)) if options.len() == 4)
        );
        let pending = project(&game, actor, "quota-committed");
        assert_eq!(pending.question().usage, wire::QuestionUse::PendingOrder);
        assert_eq!(budgets(&pending), [0, 0]);
    }
}

#[test]
fn batches_that_fit_the_quota_go_directly_to_pending_order() {
    for actor in [Viewer::P1, Viewer::P2] {
        let game = position(actor, 3, false);
        let native = question(&game, actor);
        assert!(
            matches!(&native.shape, QuestionShape::Decision(DecisionShape::Pending(choices)) if choices.len() == 6)
        );
        let pending = project(&game, actor, "quota-not-needed");
        assert_eq!(pending.question().usage, wire::QuestionUse::PendingOrder);
        assert_eq!(choices(&pending).unwrap().0.len(), 6);
    }
}

fn private_program(number: &str) -> Option<WellFormedCardProgram> {
    let mut raw = source_provenance::granting(number, true).card().clone();
    let template = program(1, "self");
    let mut phase = template.card().abilities.get(1)?.node.clone();
    phase.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    let root = raw.abilities.first_mut()?;
    let span = root.source.clone();
    let source = phase.source.clone();
    locate_steps(&mut phase.body.as_mut()?.steps, &span, &source);
    let Node::Instruction { instruction: body } = &mut phase.body.as_mut()?.steps.first_mut()?.node
    else {
        return None;
    };
    body.id = InstructionId(99);
    body.receipt = ReceiptId(99);
    let Node::Instruction { instruction } = &mut root.node.body.as_mut()?.steps.first_mut()?.node
    else {
        return None;
    };
    let Action::Grant { abilities, .. } = &mut instruction.action else {
        return None;
    };
    let GrantedAbility::Definition { ability } = abilities.first_mut()? else {
        return None;
    };
    **ability = phase;
    Some(WellFormedCardProgram::check(raw).unwrap())
}

fn private_position(actor: Viewer, number: &str, secret: &str) -> Game {
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    let them = if actor == Viewer::P1 { "P2" } else { "P1" };
    let mut initial = setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["field"] = json!([{"id":"recipient","card":"unit-follower"}]);
    initial["players"][me]["zones"]["hand"] = json!([{"id":"giver","card":number}]);
    initial["players"][them]["zones"]["field"] = json!([]);
    initial["players"][them]["zones"]["hand"] = json!([{"id":"secret","card":secret}]);
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-private-quota",
    )
    .unwrap();
    let mut static_only = program(1, "self").card().clone();
    static_only
        .abilities
        .retain(|ability| ability.node.trigger.is_none());
    for key in ["unit-follower", "unit-amulet", "unit-spell"] {
        let program = if key == "unit-follower" {
            WellFormedCardProgram::check(static_only.clone()).unwrap()
        } else {
            private_program(key).unwrap()
        };
        game.load_internal_card(key, &program).unwrap();
    }
    resolved(
        game.activate_internal_card("giver", AbilityId(1), actor.player())
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn private_quota_sources_and_opposing_secrets_preserve_both_complete_views_in_both_seats() {
    let references = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let left = private_position(actor, "unit-amulet", "unit-spell");
        let right = private_position(actor, "unit-spell", "unit-spell");
        let before = exact_authority(&left);
        let native_left = question(&left, actor);
        let native_right = question(&right, actor);
        let QuestionShape::Decision(DecisionShape::TriggerQuota(domain)) = &native_left.shape
        else {
            panic!("real private quota admission")
        };
        assert_eq!(domain.candidates.len(), 2);
        assert_ne!(
            native_left.shape, native_right.shape,
            "same hand count, different authorized occurrence identities"
        );
        let waiting = opposite(actor);
        assert!(matches!(left.input(waiting), Input::Waiting(_)));
        assert_eq!(left.input(waiting), right.input(waiting));
        let board = left.battle_observation(waiting, references).unwrap();
        assert_eq!(
            board,
            right.battle_observation(waiting, references).unwrap()
        );
        assert_eq!(board.pending.as_slice(), []);
        assert_eq!(board.processing.as_slice(), []);
        for game in [&left, &right] {
            assert!(
                game.battle_question(waiting, &QuestionId::new("waiting".into()), references)
                    .unwrap()
                    .is_none()
            );
        }
        let other_secret = private_position(actor, "unit-amulet", "unit-follower");
        let first = project(&left, actor, "private-quota");
        let different_source = project(&right, actor, "private-quota");
        assert_eq!(printing(&first).unwrap(), ("printing-amulet", 0));
        assert_eq!(printing(&different_source).unwrap(), ("printing-spell", 0));
        let second = project(&other_secret, actor, "private-quota");
        assert_eq!(first.observation(), second.observation());
        assert_eq!(first.question(), second.question());
        assert_eq!(exact_authority(&left), before);
    }
}

fn printing(projection: &QuestionProjection) -> Option<(&str, u64)> {
    let (options, _) = choices(projection)?;
    let wire::ChoiceValue::Occurrence {
        source:
            SourceRef::Card {
                printing_id,
                face: Some(face),
                ..
            },
        ..
    } = &options.first()?.value
    else {
        return None;
    };
    Some((printing_id.as_str(), face.get()))
}
