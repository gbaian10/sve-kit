//! Representative formal processing entry points; detailed rule behavior lives in its own tests.
use super::*;
use crate::player_pipeline::question;
use crate::{board_fixture, payment_fixtures};
use sve_engine::game::player::observation::ProcessingSource;
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

fn private_granted_question() -> WellFormedCardProgram {
    let mut raw = source_provenance::granting("unit-amulet", true)
        .card()
        .clone();
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
        game.observe(viewer).unwrap().processing_sources.is_empty(),
        "no unauthorized or retired frame remains"
    );
}
fn sources(game: &Game, viewer: Viewer) -> Vec<ProcessingSource> {
    game.observe(viewer).unwrap().processing_sources
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
    assert!(
        sources(&game, Viewer::P1)[0].source.holder.is_none(),
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
    let mut raw = program(json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":1}}]}))
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
    let q = game.internal_decision(View::Referee).unwrap();
    let result = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(7)))
            .unwrap(),
    );
    assert_eq!(
        result
            .progress
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
    let program = private_granted_question();
    let mut game = source_provenance::prepared_program(1, "unit-amulet", true, &program);
    source_provenance::attack(&mut game, 1, "recipient");
    let pending = question(&game, Viewer::P1);
    let selected = if let QuestionShape::Decision(DecisionShape::Pending(choices)) = &pending.shape
    {
        Some(choices[0].handle)
    } else {
        None
    };
    assert_eq!(
        submit(
            &mut game,
            Viewer::P1,
            &pending,
            Answer::Pending(selected.unwrap())
        ),
        SubmissionKind::Applied
    );
    question(&game, Viewer::P2);
    assert_eq!(sources(&game, Viewer::P1).len(), 1);
    empty(&game, Viewer::P2);
}

#[test]
fn a_known_parent_does_not_expose_unplayed_child_private_candidate_counts() {
    use super::nested_play::{self, Scenario};
    let mut boards = Vec::new();
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
        empty(&game, Viewer::P2);
        apply(&mut game, Viewer::P1, Answer::Targets(vec![0]));
        let seen = sources(&game, Viewer::P2);
        assert_eq!(
            seen.len(),
            1,
            "the actual parent played writer is independently disclosed"
        );
        assert_eq!(
            seen[0].source.card, "unit-amulet",
            "the unplayed child leaves no source slot"
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
        counts.push(members.unwrap());
        boards.push(game.observe(Viewer::P2).unwrap());
        waiting.push(game.input(Viewer::P2));
    }
    assert_eq!(
        counts,
        [1, 2],
        "the real hidden candidate sets differ with equal hand sizes"
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
