use super::*;

#[path = "card/admission.rs"]
mod admission;
use sve_engine::game::internal::{Activation, CardPlay, ResumeStage};

#[path = "card/exchange.rs"]
mod exchange;

#[path = "card/costs.rs"]
mod play_costs;

#[path = "card/payment.rs"]
mod payment;

#[path = "card/play.rs"]
mod play;

#[path = "card/fanfare.rs"]
mod fanfare;

#[path = "card/source.rs"]
mod source_information;

#[path = "card/triggers.rs"]
mod triggers;

#[path = "card/timing.rs"]
mod timing;

#[path = "card/quick.rs"]
mod quick;

#[path = "card/evolution.rs"]
mod evolution;

#[path = "card/entry.rs"]
mod entry;

#[path = "card/amulet.rs"]
mod amulet;

fn locate_steps(steps: &mut [Located<Node>], span: &SourceSpan, origin: &AbilitySource) {
    for step in steps {
        step.source = span.clone();
        match &mut step.node {
            Node::Instruction { instruction } => instruction.source = origin.clone(),
            Node::Control {
                control: Control::Sequence { body },
            } => locate_steps(&mut body.steps, span, origin),
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                locate_steps(&mut then_body.steps, span, origin);
                if let Some(body) = else_body {
                    locate_steps(&mut body.steps, span, origin);
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    locate_steps(&mut mode.body.steps, span, origin);
                }
            }
            Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {}
        }
    }
}

fn ability(id: u64, mut steps: Vec<Located<Node>>) -> Located<Ability> {
    let mut span = source("unit-follower");
    span.fragment = None;
    span.line = u32::try_from(id).unwrap();
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(id),
        },
        controller: PlayerRef::Controller {},
    };
    locate_steps(&mut steps, &span, &origin);
    Located {
        source: span,
        node: Ability {
            id: AbilityId(id),
            scope: ScopeId(id),
            source: origin,
            kind: AbilityKind::Activated,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            play: Some(PlaySpec {
                limit: None,
                captures: vec![],
                parameters: vec![],
                costs: CostSpec::Ordered { steps: vec![] },
            }),
            trigger: None,
            body: Some(body(id, steps)),
            declarations: vec![],
        },
    }
}

fn card(abilities: Vec<Located<Ability>>) -> WellFormedCardProgram {
    WellFormedCardProgram::check(CardProgram {
        abilities,
        card_play_costs: vec![],
    })
    .unwrap()
}

fn position(seat: u32, steps: Vec<Located<Node>>) -> Game {
    position_with_card(seat, &card(vec![ability(1, steps)]))
}

fn position_with_card(seat: u32, program: &WellFormedCardProgram) -> Game {
    position_with_state(seat, program, &Value::Null)
}

fn position_with_state(seat: u32, program: &WellFormedCardProgram, state: &Value) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([{"id":"d","card":"unit-spell"}]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":state}]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "card-seed").unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn activate(game: &mut Game, seat: u32) -> Activation {
    resolved(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap(),
    )
}

#[expect(
    clippy::panic,
    reason = "A synchronous fixture must complete its ability."
)]
fn resolved(play: CardPlay) -> Activation {
    let CardPlay::Resolved(activation) = play else {
        panic!("expected complete ability");
    };
    activation
}

#[test]
fn whole_card_activation_fixes_instance_and_distinct_executions_both_seats() {
    for seat in [1, 2] {
        let mut game = position(
            seat,
            vec![damage(
                1,
                RecipientSet::Leaders {
                    players: players(PlayerRelation::Opponents),
                },
                2,
            )],
        );
        let first = activate(&mut game, seat);
        assert!(first.progress.complete);
        assert_eq!(first.resume, ResumeStage::CheckTiming);
        assert_eq!(first.source.controller, PlayerId { seat });
        assert_eq!(first.source.definition, AbilityId(1));
        assert_eq!(first.source.holder.generation, Generation(0));
        assert!(
            matches!(first.progress.facts.as_slice(), [Fact::AbilityPlayed { execution: play, source }, Fact::Damage(_), Fact::AbilityResolved { execution: resolve, source: resolved }] if *play == first.execution && play == resolve && *source == first.source && source == resolved)
        );
        assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
        game.digest().unwrap_err();
        for view in [View::P1, View::P2, View::Referee] {
            assert!(game.projection(view).unwrap()["awaiting"].is_null());
        }
        let before = exact_authority(&game);
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        assert_eq!(game.legal().unwrap(), Vec::<Value>::new());
        game.decide(&json!({"do":"end-phase"}), "test").unwrap_err();
        game.execute_internal(&checked(vec![]), &source("synthetic"), context(seat))
            .unwrap_err();
        let step = game.resume_internal_card().unwrap();
        assert_eq!(step.events, Vec::<Value>::new());
        assert_eq!(game.internal_card_stage(), None);
        let second = activate(&mut game, seat);
        assert_eq!(first.source, second.source);
        assert_ne!(first.execution, second.execution);
        let step = game.resume_internal_card().unwrap();
        assert_eq!(step.events, Vec::<Value>::new());
        game.resume_internal_card().unwrap_err();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["turn"]["active"], if seat == 1 { "P1" } else { "P2" });
        assert_eq!(view["turn"]["phase"], "main");
    }
}

#[test]
fn body_resolves_before_lethal_and_deck_out_checks() {
    for seat in [1, 2] {
        let mut game = position(
            seat,
            vec![
                damage(
                    1,
                    RecipientSet::Leaders {
                        players: players(PlayerRelation::Opponents),
                    },
                    30,
                ),
                draw(2, 2),
            ],
        );
        let result = activate(&mut game, seat);
        cards(&result.progress, 2, 2, 1);
        assert!(matches!(
            result.progress.facts.last(),
            Some(Fact::AbilityResolved { .. })
        ));
        let view = game.projection(View::Referee).unwrap();
        assert_ne!(view["game"]["ended"], true);
        let step = game.resume_internal_card().unwrap();
        assert_eq!(step.events.len(), 2);
        assert!(step.events.iter().all(|event| event["kind"] == "敗北"));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["game"]["ended"], true);
        assert!(view["game"]["winner"].is_null());
    }
}

#[test]
fn complete_rollback_after_draw_and_failed_body_keeps_observations_and_producers() {
    for seat in [1, 2] {
        let mut game = position(
            seat,
            vec![
                draw(1, 1),
                draw_value(
                    2,
                    Expr::Compute(Box::new(IntOperation::Arithmetic {
                        operation: Arithmetic::Add,
                        arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
                    })),
                ),
            ],
        );
        let before = exact_authority(&game);
        let views = [View::P1, View::P2, View::Referee].map(|v| game.projection(v).unwrap());
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        assert_eq!(
            views,
            [View::P1, View::P2, View::Referee].map(|v| game.projection(v).unwrap())
        );
        assert_eq!(game.internal_card_stage(), None);
    }
}

#[test]
fn foreign_definitions_actor_and_holder_reject_atomically() {
    for seat in [1, 2] {
        let mut game = position(seat, vec![draw(1, 1)]);
        for (holder, definition, actor) in [
            ("s", 2, seat),
            ("s", 1, 3 - seat),
            ("d", 1, seat),
            ("missing", 1, seat),
            ("s", 1, 0),
        ] {
            let before = exact_authority(&game);
            game.activate_internal_card(holder, AbilityId(definition), PlayerId { seat: actor })
                .unwrap_err();
            assert_eq!(before, exact_authority(&game));
        }
    }
}

#[test]
fn provenance_face_line_card_and_instruction_origin_are_bound() {
    for seat in [1, 2] {
        for variant in 0..8 {
            let mut a = ability(1, vec![draw(1, 1)]);
            match variant {
                0 => a.source.card_no = "unit-spell".into(),
                1 => a.source.card_id = "foreign".into(),
                2 => a.source.face = "1".into(),
                3 => a.source.line = 2,
                4 => {
                    a.node.source.origin = AbilityOrigin::Printed {
                        face: FaceId("1".into()),
                        line: Nat(1),
                    }
                }
                5 => {
                    a.node.source.origin = AbilityOrigin::Printed {
                        face: FaceId("00".into()),
                        line: Nat(1),
                    }
                }
                6 => {
                    a.node.source.origin = AbilityOrigin::Printed {
                        face: FaceId("0".into()),
                        line: Nat(0),
                    }
                }
                _ => a.node.body.as_mut().unwrap().steps[0].source.card_no = "foreign".into(),
            }
            if let Node::Instruction { instruction } =
                &mut a.node.body.as_mut().unwrap().steps[0].node
            {
                instruction.source = a.node.source.clone();
            }
            let mut game = game(seat, 1);
            let before = exact_authority(&game);
            assert!(
                game.load_internal_card("unit-follower", &card(vec![a]))
                    .is_err()
            );
            assert_eq!(before, exact_authority(&game));
        }
    }
}

#[test]
fn unsupported_interactive_bodies_are_not_admitted() {
    for seat in [1, 2] {
        for (steps, expected) in [
            (vec![up_to(1, 1)], "whole-card synchronous action"),
            // An EX move outside the observed Look spine keeps the private shape gate.
            (
                vec![projections::put_ex(
                    1,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                )],
                "observed move shape",
            ),
        ] {
            let mut game = game(seat, 2);
            let before = exact_authority(&game);
            let error = game
                .load_internal_card("unit-follower", &card(vec![ability(1, steps)]))
                .unwrap_err();
            assert!(
                matches!(&error, sve_engine::EngineFailure::Unsupported(message) if message.contains(expected)),
                "{error:?}"
            );
            assert_eq!(before, exact_authority(&game));
        }
        // The observed Look-to-EX spine is now part of the whole-card subset.
        game(seat, 2)
            .load_internal_card(
                "unit-follower",
                &card(vec![ability(
                    1,
                    vec![
                        ordering::look(1, 2),
                        projections::put_ex(2, ordering::looked(1)),
                    ],
                )]),
            )
            .unwrap();
    }
}

#[test]
fn every_context_card_must_be_loaded_and_saved_typed_cards_fail_explicitly() {
    for seat in [1, 2] {
        let mut initial = setup(seat, json!([]));
        let actor = if seat == 1 { "P1" } else { "P2" };
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"][actor]["zones"]["hand"] = json!([{"id":"h","card":"unit-spell"}]);
        let mut game =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "load-seed").unwrap();
        game.load_internal_card("unit-follower", &card(vec![ability(1, vec![draw(1, 0)])]))
            .unwrap();
        let before = exact_authority(&game);
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        serde_json::to_value(&game).unwrap_err();
        assert!(
            game.load_internal_card("unit-follower", &card(vec![]))
                .is_err()
        );
        assert!(game.load_internal_card("unknown", &card(vec![])).is_err());
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        activate(&mut game, seat);
        let before = exact_authority(&game);
        assert!(
            game.load_internal_card("unit-amulet", &card(vec![]))
                .is_err()
        );
        assert_eq!(before, exact_authority(&game));
    }
}

#[test]
fn multiple_supported_abilities_keep_distinct_instances_and_effect_producers() {
    for seat in [1, 2] {
        let first = ability(1, vec![numeric::buff(1, 1, 1)]);
        let second = ability(2, vec![draw(2, 0)]);
        let mut game = position_with_card(seat, &card(vec![first, second]));
        let first = activate(&mut game, seat);
        let effects = |activation: &Activation| {
            activation
                .progress
                .facts
                .iter()
                .filter_map(|fact| match fact {
                    Fact::StatChange(fact) => {
                        assert_eq!(fact.execution, activation.execution);
                        Some(fact.effect)
                    }
                    Fact::CardPlayed { .. }
                    | Fact::CardResolved { .. }
                    | Fact::AbilityPlayed { .. }
                    | Fact::AbilityResolved { .. }
                    | Fact::Shuffled { .. }
                    | Fact::Draw(_)
                    | Fact::Damage(_)
                    | Fact::Movement(_)
                    | Fact::Posture(_) => None,
                })
                .collect::<Vec<_>>()
        };
        let first_effects = effects(&first);
        assert_eq!(first_effects.len(), 2);
        assert_eq!(first_effects[0], first_effects[1]);
        game.resume_internal_card().unwrap();
        let second = resolved(
            game.activate_internal_card("s", AbilityId(2), PlayerId { seat })
                .unwrap(),
        );
        assert_eq!(first.source.holder, second.source.holder);
        assert_ne!(first.source.instance, second.source.instance);
        assert_ne!(first.execution, second.execution);
        game.resume_internal_card().unwrap();
        let third = activate(&mut game, seat);
        assert_eq!(third.source.instance, first.source.instance);
        assert_ne!(third.execution, first.execution);
        assert_ne!(effects(&third), first_effects);
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn end_quick_window_rejection_does_not_advance_to_discard() {
    for seat in [1, 2] {
        let mut game = position(seat, vec![draw(1, 0)]);
        game.submit(&json!({"do":"end-phase"})).unwrap();
        let before = exact_authority(&game);
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        assert_eq!(game.internal_card_stage(), None);
    }
}

#[test]
fn empty_loaded_card_cannot_be_silently_omitted_from_digest() {
    for seat in [1, 2] {
        let mut game = game(seat, 1);
        game.digest().unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let before = exact_authority(&game);
        game.digest().unwrap_err();
        assert_eq!(before, exact_authority(&game));
    }
}

#[test]
fn silenced_or_nonfield_card_holder_is_not_activated() {
    for seat in [1, 2] {
        for state in [json!({"silenced":true}), json!({"card_type":"スペル"})] {
            let program = card(vec![ability(1, vec![draw(1, 0)])]);
            let mut game = position_with_state(seat, &program, &state);
            let before = exact_authority(&game);
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat })
                .unwrap_err();
            assert_eq!(before, exact_authority(&game));
        }
    }
}
