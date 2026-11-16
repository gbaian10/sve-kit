use super::super::*;
use crate::battle::build_observation;
use crate::battle::tests::{board_fixture, board_fixtures, payment_fixtures};
use crate::game::player::{Input, Question, QuestionShape, SubmissionKind, Viewer};
use crate::game::{Game, View};
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{CardProgram, PlayerId, WellFormedCardProgram, payment_expanded_tree};
use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_battle_contract::{QuestionId, Subject, Zone, observation::BattleObservation};

mod privacy;
mod roles;
mod validation;

pub(super) fn program(body: Value) -> WellFormedCardProgram {
    let document = payment_fixtures::x_document(payment_expanded_tree(), "activate", body).unwrap();
    ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

pub(super) fn board(actor: Viewer, program: &WellFormedCardProgram) -> Game {
    board_with(
        actor,
        program,
        &["unit-follower", "unit-spell"],
        &["unit-spell"],
    )
}

fn hand(cards: &[&str], prefix: &str) -> Value {
    json!(
        cards
            .iter()
            .enumerate()
            .map(|(index, card)| json!({"id":format!("{prefix}-{index}"),"card":card}))
            .collect::<Vec<_>>()
    )
}

pub(super) fn board_with(
    actor: Viewer,
    program: &WellFormedCardProgram,
    own_hand: &[&str],
    other_hand: &[&str],
) -> Game {
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["pp"] = json!({"current":3_i32,"max":3_i32});
    initial["players"][own]["zones"] = json!({
        "field":[{"id":"source","card":"unit-follower"}],
        "hand":hand(own_hand, "own"),
        "ex":[{"id":"e1","card":"unit-follower"},{"id":"e2","card":"unit-amulet"}]
        ,"deck":[{"id":"deck-first","card":"unit-follower"},{"id":"deck-last","card":"unit-spell"}]
    });
    initial["players"][other]["zones"] = json!({
        "field":[{"id":"enemy","card":"unit-follower"}],
        "hand":hand(other_hand, "hidden")
    });
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "decision-select",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game.load_internal_card("unit-follower", program).unwrap();
    game.activate_internal_card(
        "source",
        program.card().abilities.first().unwrap().node.id,
        PlayerId {
            seat: actor.player().seat,
        },
    )
    .unwrap();
    game.advance_player_input();
    game
}

pub(super) const fn seats(actor: Viewer) -> (&'static str, &'static str) {
    match actor {
        Viewer::P1 => ("P1", "P2"),
        Viewer::P2 => ("P2", "P1"),
    }
}

pub(super) fn question(game: &Game, actor: Viewer) -> Option<Question> {
    let Input::Question(question) = game.input(actor) else {
        return None;
    };
    Some(*question)
}

pub(super) fn domain(question: &Question) -> Option<&DecisionShape> {
    let QuestionShape::Decision(domain) = &question.shape else {
        return None;
    };
    Some(domain)
}

const fn selection(shape: &wire::QuestionShape) -> Option<&wire::Selection> {
    if let wire::QuestionShape::Select { selection } = shape {
        Some(selection)
    } else {
        None
    }
}

pub(super) fn components(game: &Game, actor: Viewer) -> BattleObservation {
    let fixture = board_fixture::fixture().unwrap();
    // Only board components are tested here; the public processing gate stays closed.
    build_observation(
        &fixture.catalog,
        &fixture.references,
        &game.observe(actor).unwrap(),
        vec![],
    )
    .unwrap()
}

pub(super) fn project_question(
    native: &Question,
    observation: &BattleObservation,
    id: &QuestionId,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    project(
        domain(native).ok_or(ProjectionError::Unavailable)?,
        Some(Context {
            id,
            actor: native.actor,
            purpose: native.purpose,
            observation,
        }),
    )
}

fn pick_first(shape: &wire::QuestionShape) -> Answer {
    Answer::Select {
        selected: selection(shape)
            .unwrap()
            .candidates
            .iter()
            .take(1)
            .map(|candidate| candidate.id.clone())
            .collect(),
        groups: None,
        roles: vec![],
    }
}

fn native_selection(question: &Question) -> Option<&player::SelectionQuestion> {
    if let DecisionShape::Targets(selection)
    | DecisionShape::CostMaterials(selection)
    | DecisionShape::Discard {
        cards: selection, ..
    } = domain(question)?
    {
        Some(selection)
    } else {
        None
    }
}

#[test]
fn ordinary_materials_keep_the_ex_origin_and_native_payment_channel() {
    let program = program(json!({
        "cost":{"steps":[{"pp":1_i32},{"banish":{"sel":{"who":"self","zone":"ex"},"n":1_i32,"distinct_by":"orig_cost"}}]},
        "do":[{"life":{"who":"self","delta":1_i32}}]
    }));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        assert!(matches!(
            domain(&native),
            Some(DecisionShape::CostMaterials(_))
        ));
        let (usage, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("material".into()),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::CostMaterial);
        assert_eq!(
            selection(&shape).unwrap().distinct,
            vec![wire::Distinct::OriginalCost]
        );
        assert!(
            selection(&shape)
                .unwrap()
                .candidates
                .iter()
                .all(|candidate| candidate.original_cost.is_some_and(|cost| cost.get() == 1))
        );
        assert!(
            selection(&shape)
                .unwrap()
                .candidates
                .iter()
                .all(|candidate| candidate
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.zone == Zone::Ex))
        );
        let command = binding.answer(native.token, &pick_first(&shape)).unwrap();
        assert!(matches!(
            &command,
            player::Command::Answer {
                answer: player::Answer::CostMaterials(_),
                ..
            }
        ));
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        let state = game.projection(View::Referee).unwrap();
        let (own, _) = seats(actor);
        assert_eq!(state[own]["pp"]["current"], 2_i32);
        assert_eq!(state[own]["leader"]["life"], 21_i32);
        assert_eq!(state[own]["banish"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn effect_discards_keep_the_effect_answer_channel_after_payment() {
    let program = program(json!({
        "cost":{"pp":1_i32},
        "do":[{"discard":{"who":"self","n":1_i32}},{"life":{"who":"self","delta":1_i32}}]
    }));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        assert!(matches!(
            domain(&native),
            Some(DecisionShape::Discard {
                kind: player::DiscardKind::Effect,
                ..
            })
        ));
        let (usage, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("discard".into()),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::EffectDiscard);
        let command = binding.answer(native.token, &pick_first(&shape)).unwrap();
        assert!(matches!(
            &command,
            player::Command::Answer {
                answer: player::Answer::Targets(_),
                ..
            }
        ));
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        let state = game.projection(View::Referee).unwrap();
        let (own, _) = seats(actor);
        assert_eq!(state[own]["pp"]["current"], 2_i32);
        assert_eq!(state[own]["leader"]["life"], 21_i32);
        assert_eq!(state[own]["cemetery"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn leader_targets_use_the_native_recipient_without_inventing_a_card() {
    let program = program(json!({
        "cost":{"pp":1_i32},
        "targets":[{"bind":"enemy","from":{"leader":"opp"},"n":1_i32}],
        "do":[{"damage":{"to":"enemy","n":1_i32}}]
    }));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        let (usage, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("leader".into()),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::Targets);
        assert!(matches!(
            &selection(&shape)
                .unwrap()
                .candidates
                .first()
                .unwrap()
                .subject,
            Some(Subject::Leader { .. })
        ));
        let command = binding.answer(native.token, &pick_first(&shape)).unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        let (_, other) = seats(actor);
        assert_eq!(
            game.projection(View::Referee).unwrap()[other]["leader"]["life"],
            19_i32
        );
    }
}

#[test]
fn formal_targets_keep_their_subjects_and_submit_to_the_native_stop_both_seats() {
    let program = program(json!({
        "cost":{"pp":1_i32},
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1_i32}],
        "do":[{"damage":{"to":"enemy","n":1_i32}}]
    }));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        let observation = components(&game, actor);
        assert_eq!(observation.viewer, super::super::super::common::seat(actor));
        let (usage, shape, binding) = project(
            domain(&native).unwrap(),
            Some(Context {
                id: &QuestionId::new("select".into()),
                actor,
                purpose: native.purpose,
                observation: &observation,
            }),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::Targets);
        let selection = selection(&shape).unwrap();
        assert_eq!(selection.candidates.len(), 1);
        let command = binding
            .answer(
                native.token,
                &Answer::Select {
                    selected: vec![selection.candidates.first().unwrap().id.clone()],
                    groups: None,
                    roles: vec![],
                },
            )
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            1_i32
        );
    }
}

#[test]
fn grouped_hand_costs_keep_both_groups_and_the_cost_material_answer_channel() {
    let program = program(json!({
        "cost":{"steps":[{"pp":1_i32},{"discard":{"groups":[
            {"sel":{"who":"self","zone":"hand","kind":"follower"},"n":1_i32},
            {"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1_i32}
        ]}}]},
        "do":[{"life":{"who":"self","delta":1_i32}}]
    }));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board(actor, &program);
        let native = question(&game, actor).unwrap();
        assert!(matches!(
            domain(&native),
            Some(DecisionShape::Discard {
                kind: player::DiscardKind::Cost,
                ..
            })
        ));
        let (usage, shape, binding) = project_question(
            &native,
            &components(&game, actor),
            &QuestionId::new("cost".into()),
        )
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::CostDiscard);
        let selection = selection(&shape).unwrap();
        assert_eq!(selection.groups.len(), 2);
        let command = binding
            .answer(
                native.token,
                &Answer::Select {
                    selected: selection
                        .candidates
                        .iter()
                        .rev()
                        .map(|candidate| candidate.id.clone())
                        .collect(),
                    groups: None,
                    roles: vec![],
                },
            )
            .unwrap();
        assert!(matches!(
            &command,
            player::Command::Answer {
                answer: player::Answer::CostMaterials(_),
                ..
            }
        ));
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        let state = game.projection(View::Referee).unwrap();
        let (own, _) = seats(actor);
        assert_eq!(state[own]["pp"]["current"], 2_i32);
        assert_eq!(state[own]["leader"]["life"], 21_i32);
        assert_eq!(state[own]["cemetery"].as_array().unwrap().len(), 2);
    }
}
