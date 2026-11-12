#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate rollback boundaries."
)]
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::game::player::{
    Command, DecisionShape, Input, QuestionShape, SubmissionKind, Viewer,
};

fn fragment(header: &Value, number: &str) -> WellFormedCardProgram {
    lowerer()
        .lower(&equipment_played::tree(header, number))
        .unwrap()
        .remove(number)
        .unwrap()
}

fn selected_binding(costs: &CostSpec) -> Binding<Selector> {
    let CostSpec::Ordered { steps } = costs else {
        panic!("costs")
    };
    steps
        .iter()
        .find_map(|step| {
            if let Action::Counter {
                holder:
                    Holder::Object {
                        object: ObjectOperand::Selected { selection },
                    },
                ..
            } = &step.instruction.action
            {
                Some(*selection)
            } else {
                None
            }
        })
        .unwrap()
}

fn append_departure(costs: &mut CostSpec) {
    let binding = selected_binding(costs);
    let CostSpec::Ordered { steps } = costs else {
        panic!("costs")
    };
    let mut departure = steps[0].clone();
    departure.instruction.id = InstructionId(900);
    departure.instruction.receipt = ReceiptId(900);
    departure.instruction.action = Action::Banish {
        objects: Selector::Bound { reference: binding },
    };
    let index = steps
        .iter()
        .position(|step| matches!(step.instruction.action, Action::Counter { .. }))
        .unwrap();
    steps.insert(index.checked_add(1).unwrap(), departure);
}

fn source_program() -> WellFormedCardProgram {
    let mut tree = equipment_played::tree(
        &json!({"activate":{
            "per_turn":1,
            "cost":{"steps":[{"pp":1},{"act_self":true},{"counter":{"on":{"who":"self","zone":"field","name":"unit-amulet"},"holders":1,"name":"test","n":1}}]},
            "do":[{"choice":{"n":1,"exclude_chosen":"this_turn","options":[
                [{"discard":{"who":"self","n":1,"random":true}},{"discard":{"who":"self","n":1}},{"draw":{"n":0}}],
                [{"draw":{"n":0}}]
            ]}}]
        }}),
        "unit-follower",
    );
    let dice = (0..16)
        .map(|index| json!({"dice":{"bind":format!("die{index}")}}))
        .collect::<Vec<_>>();
    let template = tree["cards"]["unit-follower"]["abilities"][0].clone();
    for (line, body) in [
        (3, json!(dice)),
        (4, json!([{"discard":{"who":"self","n":1,"random":true}}])),
    ] {
        let mut entry = template.clone();
        entry["line"] = json!(line);
        entry["activate"] = json!({"cost":{"pp":0}, "do":body});
        tree["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(entry);
    }
    let mut raw = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    append_departure(&mut raw.abilities[0].node.play.as_mut().unwrap().costs);
    let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("modes")
    };
    let Node::Instruction { instruction } = &mut modes[0].body.steps[2].node else {
        panic!("final draw")
    };
    let Action::Draw { count, .. } = &mut instruction.action else {
        panic!("draw amount")
    };
    *count = Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation: Arithmetic::Add,
        arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
    }));
    WellFormedCardProgram::check(raw).unwrap()
}

fn board(program: &WellFormedCardProgram) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"] = json!({"field":[{"id":"source","card":"unit-follower"},{"id":"pool","card":"unit-amulet","state":{"counters":{"test":1}}}],
        "hand":[{"id":"h0","card":"unit-spell"},{"id":"h1","card":"unit-spell"},{"id":"h2","card":"unit-spell"}]});
    initial["players"]["P2"]["zones"] = json!({});
    let observer = fragment(
        &json!({"auto":{"zone":"hand","on":{"discarded":{"ref":"this","who":"self","from":"hand"}},"do":[{"life":{"who":"self","delta":1}}]}}),
        "unit-spell",
    );
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "payment-recovery",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &observer).unwrap();
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("awaiting transaction")
    };
    q
}

#[test]
fn payment_failure_after_body_rng_restores_material_generation_history_pending_modes_and_quota() {
    let program = source_program();
    let probe = |line| {
        program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.source.line == line)
            .unwrap()
            .node
            .id
    };
    let dice = probe(3);
    let discard = probe(4);
    let mut game = board(&program);
    let origin = game.clone();
    let mut chosen = None;
    for _ in 0..2 {
        let modes = awaiting(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Modes {
            modes: available, ..
        } = &modes.domain
        else {
            panic!("modes")
        };
        let first = *chosen.get_or_insert(available[0]);
        assert!(
            available.contains(&first),
            "rollback makes the previously used mode available"
        );
        let materials = awaiting(
            game.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![first]))
                .unwrap(),
        );
        let InputDomain::CostMaterials(domain) = &materials.domain else {
            panic!("materials")
        };
        let hand = awaiting(
            game.answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let paid = game.projection(View::Referee).unwrap();
        assert_eq!(paid["P1"]["pp"]["current"], 1);
        assert_eq!(paid["objects"]["source"]["acted"], true);
        assert_eq!(paid["P1"]["banish"], json!(["pool"]));
        assert_eq!(
            paid["P1"]["hand"].as_array().unwrap().len(),
            2,
            "the body has already discarded one random card"
        );
        let InputDomain::Targets(domain) = &hand.domain else {
            panic!("body discard")
        };
        let published = game.clone();
        assert!(matches!(
            game.answer_internal_card(
                hand.actor,
                hand.token,
                Answer::Targets(vec![domain.members[0].handle])
            ),
            Err(EngineFailure::Invalid(_) | EngineFailure::Invariant(_))
        ));
        transactions::assert_reverted_from(&game, &origin, &published);
        assert_random_continuation(&game, &origin, dice, discard);
    }
}

fn dice_sequence(game: &mut Game, ability: AbilityId) -> Vec<DieFace> {
    let played = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    let result = resolved(played);
    let dice = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Dice(event) = fact {
                Some(event.value)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        dice.len(),
        16,
        "the probe observes sixteen successive native random results"
    );
    dice
}

fn assert_random_continuation(restored: &Game, origin: &Game, dice: AbilityId, discard: AbilityId) {
    let mut control = origin.clone();
    let expected = dice_sequence(&mut control, dice);
    let mut actual = restored.clone();
    assert_eq!(
        dice_sequence(&mut actual, dice),
        expected,
        "failure restores the subsequent random stream"
    );

    let mut advanced = origin.clone();
    let prefix = advanced
        .activate_internal_card("source", discard, PlayerId { seat: 1 })
        .unwrap();
    resolved(prefix);
    loop {
        while advanced.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            advanced.resume_internal_card().unwrap();
        }
        let Some(q) = advanced.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("discard observer")
        };
        let observed = advanced
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        resolved(observed);
    }
    assert_ne!(
        dice_sequence(&mut advanced, dice),
        expected,
        "the same committed random-discard prefix changes this seed's continuation, so an opaque RNG Debug cannot hide a missed rollback"
    );
}

fn cancelled_head() -> (Game, DecisionView) {
    let mut raw=fragment(&json!({"fanfare":{"cost":{"steps":[{"pp":1},{"counter":{"on":{"who":"self","zone":"field","name":"unit-amulet"},"holders":1,"name":"test","n":1}},{"leader_life":21}]},"do":[]}}),"unit-follower").card().clone();
    let Node::Control {
        control: Control::Pay { costs, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("head payment")
    };
    append_departure(costs);
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"] = json!({"hand":[{"id":"source","card":"unit-follower"}],"field":[{"id":"pool","card":"unit-amulet","state":{"counters":{"test":1}}}]});
    initial["players"]["P2"]["zones"] = json!({});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "head-cancel",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("pending")
    };
    let q = awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    (game, q)
}

#[test]
fn a_failed_required_head_payment_matches_decline_without_reviving_the_occurrence() {
    let (mut game, q) = cancelled_head();
    assert_eq!(q.domain, InputDomain::OptionalCost);
    let mut declined = game.clone();
    assert!(matches!(
        declined
            .answer_internal_card(q.actor, q.token, Answer::OptionalCost(false))
            .unwrap(),
        CardPlay::Cancelled
    ));
    let mut result = game
        .answer_internal_card(q.actor, q.token, Answer::OptionalCost(true))
        .unwrap();
    if let CardPlay::Awaiting(materials) = result {
        let InputDomain::CostMaterials(domain) = &materials.domain else {
            panic!("head materials")
        };
        result = game
            .answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    assert!(matches!(result, CardPlay::Cancelled));
    transactions::assert_restored(&game, &declined);
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "cancellation consumes the occurrence"
    );
}

#[test]
fn random_costs_are_rejected_before_any_payment_or_revelation() {
    for quick in [false, true] {
        let mut header = json!({"activate":{"cost":{"pp":0},"do":[{"discard":{"who":"self","n":1,"random":true}}]}});
        if quick {
            header["activate"]["quick"] = json!(true);
        }
        let mut raw = fragment(&header, "unit-follower").card().clone();
        let Node::Instruction { instruction } = raw.abilities[0]
            .node
            .body
            .as_mut()
            .unwrap()
            .steps
            .remove(0)
            .node
        else {
            panic!("random discard")
        };
        let CostSpec::Ordered { steps } = &mut raw.abilities[0].node.play.as_mut().unwrap().costs
        else {
            panic!("cost")
        };
        steps[0].instruction.action = instruction.action;
        let program = WellFormedCardProgram::check(raw).unwrap();
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "random-cost",
        )
        .unwrap();
        let before = exact_authority(&game);
        assert!(matches!(
            game.load_internal_card("unit-follower", &program),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn player_payment_errors_are_reason_free_and_keep_rejected_reverted_and_fault_distinct() {
    use crate::player_pipeline::{answer, question};
    let mut raw = quick_counter::stale_program().card().clone();
    raw.abilities.truncate(1);
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut game = quick_counter::load(&quick_counter::initial(true, 2, false), &program);
    game.advance_player_input();
    let command = player_declarations::activation_command(&game, "unit-amulet", 0);
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied
    );
    let q = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::CostMaterials(materials)) = &q.shape else {
        panic!("player cost materials")
    };
    let material = materials.members[0].handle;
    let before = exact_authority(&game);
    assert_eq!(
        game.submit_player(
            Viewer::P2,
            Command::Answer {
                token: q.token,
                answer: Answer::CostMaterials(vec![0])
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::CostMaterials(vec![99])).kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::CostMaterials(vec![material])).kind,
        SubmissionKind::Reverted
    );
    let restored = game.projection(View::Referee).unwrap();
    assert_eq!(restored["P1"]["pp"]["current"], 2);
    assert_eq!(restored["objects"]["source"]["acted"], false);
    assert_eq!(restored["P1"]["banish"], json!([]));
    let bad = card(vec![payment::paid_ability(
        vec![payment::pp(1)],
        vec![draw_value(
            1,
            Expr::Compute(Box::new(IntOperation::Arithmetic {
                operation: Arithmetic::Add,
                arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
            })),
        )],
    )]);
    let mut fault = position_with_card(1, &bad);
    fault.advance_player_input();
    let command = player_declarations::activation_command(&fault, "unit-follower", 0);
    let result = fault.submit_player(Viewer::P1, command);
    assert_eq!(result.kind, SubmissionKind::EngineFault);
    assert!(matches!(result.input, Input::EngineFault));
    for viewer in [Viewer::P1, Viewer::P2] {
        assert!(matches!(fault.input(viewer), Input::EngineFault));
    }
    assert_eq!(
        fault.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2
    );
    let (mut cancel, _decline) = cancelled_head();
    cancel.advance_player_input();
    assert_eq!(
        answer(&mut cancel, Viewer::P1, Answer::OptionalCost(false)).kind,
        SubmissionKind::Cancelled
    );
}
