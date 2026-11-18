//! Representative formal processing entry points; detailed rule behavior lives in its own tests.
use super::*;
use crate::player_pipeline::question;
use crate::{board_fixture, payment_fixtures};
use sve_battle_contract::{QuestionId, Seat, observation as wire};
use sve_engine::game::player::observation::{
    PaymentKind, ProcessingContext, ProcessingDeclaration, ProcessingStage, ProcessingSubject,
};
use sve_engine::game::player::{
    Command, DecisionShape, Question, QuestionShape, SubmissionKind, Viewer,
};

fn program(body: Value) -> WellFormedCardProgram {
    let document =
        payment_fixtures::x_document(fanfare::expanded::tree(), "activate", body).unwrap();
    lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn fixture(program: &WellFormedCardProgram) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":3,"max":3});
    initial["players"]["P1"]["zones"] = json!({"field":[{"id":"source","card":"unit-follower"}]});
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"a","card":"unit-follower"},{"id":"b","card":"unit-follower"}
    ]);
    let fixture = board_fixture::fixture().unwrap();
    let mut game = Game::new(
        Arc::clone(&fixture.catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "processing",
    )
    .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.load_internal_card("unit-follower", program).unwrap();
    game
}

fn submit(game: &mut Game, actor: Viewer, q: &Question, answer: Answer) -> SubmissionKind {
    game.submit_player(
        actor,
        Command::Answer {
            token: q.token,
            answer,
        },
    )
    .kind
}

fn finish_may(game: &mut Game) -> Progress {
    let q = game.internal_decision(View::Referee).unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(0)))
            .unwrap(),
    )
    .progress
}

fn with_overflow(program: &WellFormedCardProgram) -> WellFormedCardProgram {
    let mut raw = program.card().clone();
    let changed = if let Node::Control {
        control: Control::May { body, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        && let Node::Instruction { instruction } = &mut body.steps[0].node
        && let Action::Life {
            change: NumericChange::Add { amount },
            ..
        } = &mut instruction.action
    {
        *amount = Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::Add,
            arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
        }));
        true
    } else {
        false
    };
    assert!(
        changed,
        "the admitted optional body contains the intended failing life operation"
    );
    WellFormedCardProgram::check(raw).unwrap()
}

fn opponent() -> PlayerRef {
    PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        }),
    }
}

fn private_granted_question(number: &str) -> WellFormedCardProgram {
    let mut raw = source_provenance::granting(number, true).card().clone();
    let nested = if let Node::Instruction { instruction } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        && let Action::Grant { abilities, .. } = &mut instruction.action
        && let GrantedAbility::Definition { ability } = &mut abilities[0]
    {
        Some(ability)
    } else {
        None
    };
    assert!(
        nested.is_some(),
        "the fixture contains the original granted definition"
    );
    let nested = nested.unwrap();
    let body = nested.body.as_mut().unwrap();
    let step = &mut body.steps[0];
    let changed = if let Node::Instruction { instruction } = &mut step.node {
        instruction.action = Action::Declare {
            actor: opponent(),
            declaration: Declaration::NonnegativeInteger {
                binding: Binding::new(1000, body.scope),
            },
        };
        true
    } else {
        false
    };
    assert!(changed, "the real granted attack body now asks a number");
    WellFormedCardProgram::check(raw).unwrap()
}

fn activate(game: &mut Game, program: &WellFormedCardProgram) {
    game.activate_internal_card(
        "source",
        program.card().abilities[0].node.id,
        Viewer::P1.player(),
    )
    .unwrap();
}
fn apply(game: &mut Game, actor: Viewer, answer: Answer) {
    let q = question(game, actor);
    assert_eq!(
        submit(game, actor, &q, answer),
        SubmissionKind::Applied,
        "the fixture accepts its current answer"
    );
}
fn empty(game: &Game, viewer: Viewer) {
    assert!(
        game.observe(viewer).unwrap().processing.is_empty(),
        "no unauthorized or retired frame remains"
    );
}
fn sources(game: &Game, viewer: Viewer) -> Vec<ProcessingContext> {
    game.observe(viewer)
        .unwrap()
        .processing
        .into_iter()
        .filter(|context| context.source.is_some())
        .collect()
}

#[test]
fn a_public_play_discloses_its_source_to_an_unasked_seat() {
    let program = program(
        json!({"cost":{"pp":0},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    );
    let mut game = fixture(&program);
    activate(&mut game, &program);
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            sources(&game, viewer).len(),
            1,
            "the actual played writer discloses the public source independently of journal delivery"
        );
    }
    finish_may(&mut game);
    empty(&game, Viewer::P2);
}

#[test]
fn rejected_answers_and_failed_bodies_preserve_the_correct_play_boundary() {
    let program = with_overflow(&program(
        json!({"cost":{"steps":[{"pp":1},{"self_to_cemetery":true}]},
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    ));
    let mut game = fixture(&program);
    activate(&mut game, &program);
    let mine = sources(&game, Viewer::P1);
    let theirs = sources(&game, Viewer::P2);
    assert_eq!(mine[0].stage, Some(ProcessingStage::Target));
    assert_eq!(theirs[0].stage, Some(ProcessingStage::Declare));
    assert!(
        theirs[0].payments.is_empty(),
        "declaration is not a receipt"
    );
    let q = question(&game, Viewer::P1);
    assert_eq!(
        submit(&mut game, Viewer::P2, &q, Answer::Targets(vec![0])),
        SubmissionKind::Rejected
    );
    assert_eq!(
        question(&game, Viewer::P1).token,
        q.token,
        "rejection retains the outstanding question"
    );
    apply(&mut game, Viewer::P1, Answer::Targets(vec![0]));
    let paid = sources(&game, Viewer::P2);
    assert_eq!(paid[0].stage, Some(ProcessingStage::Resolve));
    assert_eq!(
        paid[0]
            .payments
            .iter()
            .map(|payment| (&payment.kind, payment.amount))
            .collect::<Vec<_>>(),
        [(&PaymentKind::Pp, 1), (&PaymentKind::Move, 1)]
    );
    assert!(
        matches!(paid[0].declared.as_slice(), [ProcessingDeclaration::Targets { targets, .. }] if matches!(targets.as_slice(), [ProcessingSubject::Card { object: Some(_), .. }]))
    );
    assert!(
        matches!(
            &paid[0].subject,
            Some(ProcessingSubject::Card { object: None, .. })
        ),
        "payment retires the subject link without losing its declared face"
    );
    assert!(
        sources(&game, Viewer::P1)[0]
            .source
            .as_ref()
            .unwrap()
            .holder
            .is_none(),
        "the frozen source does not reconnect after its cost moves it"
    );
    let q = question(&game, Viewer::P1);
    assert_eq!(
        submit(&mut game, Viewer::P1, &q, Answer::Integer(Nat(1))),
        SubmissionKind::EngineFault
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3,
        "a body fault restores the play's payment"
    );
    empty(&game, Viewer::P1);
    empty(&game, Viewer::P2);
}

#[test]
fn an_effect_number_records_the_actual_declarer() {
    let mut raw = program(
        json!({"cost":{"pp":0},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}),
    )
    .card()
    .clone();
    let ability = &mut raw.abilities[0];
    let binding = Binding::new(999, ability.node.body.as_ref().unwrap().scope);
    let mut step = instruction(
        999,
        Action::Declare {
            actor: opponent(),
            declaration: Declaration::NonnegativeInteger { binding },
        },
    );
    step.source = ability.source.clone();
    if let Node::Instruction { instruction } = &mut step.node {
        instruction.source = ability.node.source.clone();
    }
    ability.node.body.as_mut().unwrap().steps.insert(0, step);
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut game = fixture(&program);
    activate(&mut game, &program);
    let public = sources(&game, Viewer::P2);
    assert_eq!(
        (public[0].controller, public[0].actor),
        (Viewer::P1, Some(Viewer::P2))
    );
    let projection = game
        .battle_question(
            Viewer::P2,
            &QuestionId::new("declarer".into()),
            &board_fixture::fixture().unwrap().references,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            projection.question().processing.controller,
            projection.question().processing.actor
        ),
        (Some(Seat::P1), Some(Seat::P2))
    );
    let command = projection
        .answer(
            Viewer::P2,
            &projection.question().id,
            &sve_battle_contract::Answer::Integer {
                value: 7_u32.into(),
            },
        )
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P2, command).kind,
        SubmissionKind::Applied
    );
    assert!(
        sources(&game, Viewer::P2)[0]
            .declared
            .contains(&ProcessingDeclaration::Number {
                actor: Viewer::P2,
                value: 7
            })
    );
    let progress = finish_may(&mut game);
    assert_eq!(
        progress
            .facts
            .iter()
            .find(|fact| matches!(fact, Fact::Declared { .. })),
        Some(&Fact::Declared {
            player: Viewer::P2.player(),
            value: Nat(7)
        })
    );
}

#[test]
fn a_public_holder_does_not_authorize_its_private_wording_owner() {
    let references = base_references().unwrap();
    for controller in [Viewer::P1, Viewer::P2] {
        let actor = if controller == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut views = Vec::new();
        for number in ["unit-amulet", "unit-spell"] {
            let program = private_granted_question(number);
            let mut game = source_provenance::prepared_program(
                controller.player().seat,
                number,
                true,
                &program,
            );
            source_provenance::attack(&mut game, controller.player().seat, "recipient");
            let pending = question(&game, controller);
            let QuestionShape::Decision(DecisionShape::Pending(choices)) = &pending.shape else {
                panic!("a real granted occurrence")
            };
            let selected = choices[0].handle;
            assert_eq!(
                submit(&mut game, controller, &pending, Answer::Pending(selected)),
                SubmissionKind::Applied
            );
            question(&game, actor);
            assert_eq!(sources(&game, controller).len(), 1);
            empty(&game, actor);
            let projection = game
                .battle_question(actor, &QuestionId::new("opaque".into()), &references)
                .unwrap()
                .unwrap();
            assert!(
                projection.observation().processing.is_empty(),
                "a hidden frame leaves no board slot"
            );
            assert!(projection.question().processing.source.is_none());
            assert_eq!(projection.question().processing.stage, None);
            views.push((
                projection.observation().clone(),
                projection.question().clone(),
                game.input(controller),
            ));
            assert!(
                game.battle_question(controller, &projection.question().id, &references)
                    .unwrap()
                    .is_none()
            );
            let command = projection
                .answer(
                    actor,
                    &projection.question().id,
                    &sve_battle_contract::Answer::Integer {
                        value: 0_u32.into(),
                    },
                )
                .unwrap();
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
        }
        assert_eq!(
            views[0], views[1],
            "hidden wording owners preserve the complete question, board and waiting summary in both directions"
        );
    }
}

#[test]
fn a_known_parent_does_not_expose_unplayed_child_private_candidate_counts() {
    use super::nested_play::{self, Scenario};
    let mut boards = Vec::new();
    let mut wire_boards = Vec::new();
    let mut waiting = Vec::new();
    let mut counts = Vec::new();
    for eligible in [1, 2] {
        let (mut game, definition) = nested_play::build_position(
            "self",
            "unit-spell",
            true,
            Scenario::MandatoryReveal { material: true },
            |initial| {
                initial["players"]["P1"]["zones"]["hand"] = json!((0..2).map(|index| json!({"id":format!("h{index}"),"card":if index<eligible {"unit-follower"} else {"unit-spell"}})).collect::<Vec<_>>());
            },
        );
        game.activate_internal_card("parent", definition, Viewer::P1.player())
            .unwrap();
        assert_eq!(
            sources(&game, Viewer::P2).len(),
            1,
            "formal public declaration discloses the parent before its targets are answered"
        );
        apply(&mut game, Viewer::P1, Answer::Targets(vec![0]));
        let seen = sources(&game, Viewer::P2);
        assert_eq!(
            seen.len(),
            2,
            "the parent played and child formally declared public sources are independently disclosed"
        );
        assert_eq!(
            seen[0].source.as_ref().unwrap().card,
            "unit-amulet",
            "the unplayed child leaves no source slot"
        );
        assert_eq!(
            seen[1].stage,
            Some(ProcessingStage::Declare),
            "the waiting seat sees only the child's public milestone"
        );
        let q = question(&game, Viewer::P1);
        let members = if let QuestionShape::Decision(DecisionShape::CostMaterials(domain)) = q.shape
        {
            Some(domain.members.len())
        } else {
            None
        };
        assert!(
            members.is_some(),
            "the child's actual private reveal material question is pending"
        );
        let projection = game
            .battle_question(
                Viewer::P1,
                &QuestionId::new("nested".into()),
                &base_references().unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(projection.observation().processing.len(), 2);
        assert_eq!(
            projection.question().processing,
            projection.observation().processing[1],
            "the question belongs to the declared child, not its suspended parent"
        );
        assert_eq!(
            projection.question().processing.stage,
            Some(wire::ProcessingStage::Declare)
        );
        wire_boards.push(
            game.battle_observation(Viewer::P2, &base_references().unwrap())
                .unwrap(),
        );
        counts.push(members.unwrap());
        boards.push(game.observe(Viewer::P2).unwrap());
        waiting.push(game.input(Viewer::P2));
    }
    assert_eq!(
        counts,
        [1, 2],
        "the real hidden candidate sets differ with equal hand sizes"
    );
    assert_eq!(
        wire_boards[0], wire_boards[1],
        "the complete wire board hides child candidates"
    );
    assert!(
        boards[0] == boards[1],
        "the entire known-parent native board ignores the private child candidate count"
    );
    assert!(
        waiting[0] == waiting[1],
        "the waiting summary ignores the private child candidate count"
    );
}

#[test]
fn a_private_selection_does_not_change_the_other_seats_processing_view() {
    let program = program(json!({"cost":{"pp":0},"do":[
        {"choose":{"bind":"private","from":{"who":"self","zone":"hand","kind":"follower"},"upto":1}},
        {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
    ]}));
    for actor in [Viewer::P1, Viewer::P2] {
        let (me, other) = if actor == Viewer::P1 {
            ("P1", Viewer::P2)
        } else {
            ("P2", Viewer::P1)
        };
        let mut views = Vec::new();
        let mut shapes = Vec::new();
        for follower in [false, true] {
            let mut initial = setup(actor.player().seat, json!([]));
            initial["turn"]["active"] = json!(me);
            initial["players"][me]["pp"] = json!({"current":3,"max":3});
            initial["players"][me]["zones"]["field"] =
                json!([{"id":"source","card":"unit-follower"}]);
            initial["players"][me]["zones"]["hand"] =
                json!([{"id":"secret","card":if follower {"unit-follower"} else {"unit-spell"}}]);
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "private-processing",
            )
            .unwrap();
            game.load_internal_card("unit-follower", &program).unwrap();
            game.load_internal_card("unit-spell", &card(vec![]))
                .unwrap();
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                actor.player(),
            )
            .unwrap();
            shapes.push(question(&game, actor).shape);
            views.push((game.observe(other).unwrap(), game.input(other)));
        }
        assert_ne!(
            shapes[0], shapes[1],
            "the actor sees different private candidate sets"
        );
        assert_eq!(
            views[0], views[1],
            "a private selection does not change the complete waiting view"
        );
    }
}

#[test]
fn optional_acceptance_is_a_declaration_until_the_actual_debits() {
    use sve_engine::game::player::observation::PaymentPurpose;
    let program = program(json!({
        "cost":{"pp":1},
        "targets":[{"bind":"enemy","option":1,"from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"choice":{"n":1,"options":[
            [{"pay":{"cost":{"pp":1},"do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]}}],
            [{"life":{"who":"self","delta":1}}]
        ]}}]
    }));
    let mut game = fixture(&program);
    activate(&mut game, &program);
    apply(&mut game, Viewer::P1, Answer::Modes(vec![ModeId(1)]));
    apply(&mut game, Viewer::P1, Answer::OptionalCost(true));
    let declared = sources(&game, Viewer::P2);
    assert!(
        declared[0]
            .declared
            .contains(&ProcessingDeclaration::OptionalCost {
                actor: Viewer::P1,
                required: false,
                accepted: true,
            })
    );
    assert!(
        declared[0].payments.is_empty(),
        "the formal target question still precedes both debits"
    );
    apply(&mut game, Viewer::P1, Answer::Targets(vec![0]));
    let paid = sources(&game, Viewer::P2);
    assert_eq!(
        paid[0]
            .payments
            .iter()
            .map(|payment| (payment.purpose, payment.amount))
            .collect::<Vec<_>>(),
        [
            (Some(PaymentPurpose::Original), 1),
            (Some(PaymentPurpose::Optional { required: false }), 1),
        ],
        "each writer's debit appears once with its actual cost group"
    );
}

#[test]
fn the_payer_keeps_its_receipt_when_the_other_player_is_asked_next() {
    use sve_engine::game::player::observation::PaymentPurpose;
    let tree = evolution_linked_movement::tree(&[(
        "auto",
        json!({
            "on":{"phase":{"whose":"self","which":"end"}},
            "do":[{"pay":{"cost":{"pp":1},"do":[{"life":{"who":"self","delta":1}}]}}]
        }),
    )]);
    let mut raw = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    let ability = &mut raw.abilities[0];
    let binding = Binding::new(999, ability.node.body.as_ref().unwrap().scope);
    let mut step = instruction(
        999,
        Action::Declare {
            actor: opponent(),
            declaration: Declaration::NonnegativeInteger { binding },
        },
    );
    step.source = ability.source.clone();
    if let Node::Instruction { instruction } = &mut step.node {
        instruction.source = ability.node.source.clone();
    }
    ability.node.body.as_mut().unwrap().steps.push(step);
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut game = fixture(&program);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    game.advance_player_input();
    apply(&mut game, Viewer::P1, Answer::Pending(1));
    game.advance_player_input();
    apply(&mut game, Viewer::P1, Answer::OptionalCost(true));
    question(&game, Viewer::P2);
    let wire = game
        .battle_observation(Viewer::P1, &board_fixture::fixture().unwrap().references)
        .unwrap();
    assert_eq!(
        wire.processing[0].stage,
        Some(wire::ProcessingStage::Resolve),
        "the waiting payer retains the public milestone"
    );
    assert_eq!(wire.processing[0].actor, None);
    assert!(
        wire.processing[0]
            .payments
            .iter()
            .any(|payment| payment.actor == Seat::P1
                && payment.purpose == Some(wire::PaymentPurpose::Resolution)
                && payment.amount.get() == 1)
    );
    let mine = sources(&game, Viewer::P1);
    assert_eq!(mine[0].actor, None, "the payer is now waiting");
    assert!(
        mine[0]
            .payments
            .iter()
            .any(|payment| payment.actor == Viewer::P1
                && payment.purpose == Some(PaymentPurpose::Resolution)
                && payment.amount == 1),
        "changing the question actor does not revoke the payer's completed receipt"
    );
}
