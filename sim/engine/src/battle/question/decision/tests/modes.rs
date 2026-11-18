use super::super::*;
use super::select::{
    board, components, domain, program, project_game, project_question, question, seats,
};
use crate::game::View;
use crate::game::player::{Input, SubmissionKind};
use crate::ir::{self, ModeId, WellFormedCardProgram};
use serde_json::json;
use sve_battle_contract::QuestionId;

fn modes_program() -> WellFormedCardProgram {
    let mut card = program(
        json!({"cost":{"pp":0_i32},"do":[{"choice":{"n":2_i32,"options":[
            [{"life":{"who":"self","delta":1_i32}}],
            [{"life":{"who":"self","delta":2_i32}}],
            [{"life":{"who":"self","delta":3_i32}}]
        ]}}]}),
    )
    .card()
    .clone();
    let ability = &mut card.abilities.first_mut().unwrap().node;
    if let ir::Node::Control {
        control: ir::Control::Mode { modes, chosen, .. },
    } = &mut ability
        .body
        .as_mut()
        .unwrap()
        .steps
        .first_mut()
        .unwrap()
        .node
    {
        let ids = vec![ModeId(9), ModeId(2), ModeId(7)];
        for (mode, id) in modes.iter_mut().zip(&ids) {
            mode.id = *id;
        }
        if let ir::ChoiceDomain::Modes { modes: offered, .. } = &mut chosen.domain {
            *offered = ids;
        }
    }
    WellFormedCardProgram::check(card).unwrap()
}

#[test]
fn declaration_modes_preserve_printed_ids_order_count_and_native_answers() {
    let program = modes_program();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        assert!(
            matches!(domain(&native), Some(DecisionShape::Modes { modes, count: ir::Nat(2), .. }) if *modes == vec![ModeId(9), ModeId(2), ModeId(7)])
        );
        let (usage, shape, binding) =
            project_game(&game, actor, &QuestionId::new("modes".into())).unwrap();
        assert_eq!(usage, wire::QuestionUse::Modes);
        let (options, bounds, count) = if let wire::QuestionShape::Choose {
            options,
            bounds,
            count,
            ..
        } = shape
        {
            Some((options, bounds, count))
        } else {
            None
        }
        .unwrap();
        assert_eq!(count.unwrap().get(), 2);
        assert_eq!((bounds.min.get(), bounds.max.unwrap().get()), (2, 2));
        assert_eq!(
            options
                .iter()
                .map(
                    |option| if let wire::ChoiceValue::Mode { mode, .. } = option.value {
                        Some(mode.get())
                    } else {
                        None
                    }
                )
                .collect::<Vec<_>>(),
            vec![Some(9), Some(2), Some(7)]
        );
        let chosen = vec![
            options.last().unwrap().id.clone(),
            options.first().unwrap().id.clone(),
        ];
        let command = binding
            .answer(
                native.actor,
                &binding.question().id,
                &Answer::Choose { chosen },
            )
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
            24_i32
        );
        assert!(!matches!(game.input(actor), Input::EngineFault));
    }
}

#[test]
fn a_resolution_alternative_keeps_its_use_and_one_mode_answer() {
    let program = program(json!({"cost":{"pp":0_i32},"do":[{"either":{"options":[
        [{"life":{"who":"self","delta":1_i32}}],
        [{"life":{"who":"self","delta":3_i32}}]
    ]}}]}));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        assert!(matches!(
            domain(&native),
            Some(DecisionShape::ResolutionAlternative { .. })
        ));
        let (usage, shape, binding) =
            project_game(&game, actor, &QuestionId::new("alternative".into())).unwrap();
        assert_eq!(usage, wire::QuestionUse::ResolutionAlternative);
        let options = if let wire::QuestionShape::Choose {
            options,
            bounds,
            count,
            ..
        } = shape
        {
            assert_eq!(
                (
                    bounds.min.get(),
                    bounds.max.unwrap().get(),
                    count.unwrap().get()
                ),
                (1, 1, 1)
            );
            Some(options)
        } else {
            None
        }
        .unwrap();
        let chosen = vec![options.last().unwrap().id.clone()];
        let command = binding
            .answer(
                native.actor,
                &binding.question().id,
                &Answer::Choose { chosen },
            )
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
            23_i32
        );
    }
}

mod privacy;

#[test]
fn a_native_pruned_mode_set_keeps_count_larger_than_available() {
    let mut card = program(
        json!({"cost":{"pp":0_i32},"do":[{"choice":{"upto":2_i32,"options":[
            [{"life":{"who":"self","delta":1_i32}}],
            [{"life":{"who":"self","delta":3_i32}}]
        ]}}]}),
    )
    .card()
    .clone();
    let ability = &mut card.abilities.first_mut().unwrap().node;
    let mut fee = ability.play.as_ref().unwrap().costs.clone();
    if let ir::CostSpec::Ordered { steps } = &mut fee {
        let step = steps.first_mut().unwrap();
        step.instruction.id = ir::InstructionId(900);
        step.instruction.receipt = ir::ReceiptId(900);
        if let ir::Action::Resource { amount, .. } = &mut step.instruction.action {
            *amount = ir::Expr::Literal(ir::Int(9));
        }
    }
    if let ir::Node::Control {
        control: ir::Control::Mode { modes, .. },
    } = &mut ability
        .body
        .as_mut()
        .unwrap()
        .steps
        .first_mut()
        .unwrap()
        .node
    {
        modes.first_mut().unwrap().costs = fee;
    }
    let program = WellFormedCardProgram::check(card).unwrap();
    let mut game = board(Viewer::P1, &program);
    let native = question(&game, Viewer::P1).unwrap();
    assert!(
        matches!(domain(&native), Some(DecisionShape::Modes { modes, count: ir::Nat(2), .. }) if modes.len() == 1)
    );
    let (_, shape, binding) = project_question(
        &native,
        &components(&game, Viewer::P1),
        &QuestionId::new("pruned".into()),
    )
    .unwrap();
    let options = if let wire::QuestionShape::Choose {
        options,
        bounds,
        count,
        ..
    } = shape
    {
        assert_eq!(
            (
                bounds.min.get(),
                bounds.max.unwrap().get(),
                count.unwrap().get()
            ),
            (1, 1, 2)
        );
        Some(options)
    } else {
        None
    }
    .unwrap();
    let command = binding
        .answer(
            native.token,
            &Answer::Choose {
                chosen: vec![options.first().unwrap().id.clone()],
            },
        )
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23_i32
    );
}

mod validation;

#[test]
fn zero_native_mode_count_skips_the_mode_question() {
    let mut card = modes_program().card().clone();
    if let ir::Node::Control {
        control: ir::Control::Mode { count, chosen, .. },
    } = &mut card
        .abilities
        .first_mut()
        .unwrap()
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .first_mut()
        .unwrap()
        .node
    {
        *count = ir::Expr::Literal(ir::Nat(0));
        if let ir::ChoiceDomain::Modes { cardinality, .. } = &mut chosen.domain {
            *cardinality = ir::Cardinality {
                minimum: ir::Nat(0),
                maximum: Some(ir::Nat(0)),
            };
        }
    }
    let program = WellFormedCardProgram::check(card).unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let game = board(actor, &program);
        assert!(
            !question(&game, actor)
                .is_some_and(|q| matches!(domain(&q), Some(DecisionShape::Modes { .. })))
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
            20_i32
        );
    }
}
