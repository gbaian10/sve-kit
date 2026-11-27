//! Scenario helpers shared by the English-only card batches.
use super::admission_bp_cp_fixture::{finish, settle};
use super::*;

pub(super) fn state(board: &Game) -> Value {
    board.projection(View::Referee).unwrap()
}

/// Answers unbounded target choices with every candidate; other questions use the fixture.
pub(super) fn take_all(board: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = &step {
        let InputDomain::Targets(domain) = &question.domain else {
            finish(board, step);
            return;
        };
        let chosen = domain.members.iter().map(|member| member.handle).collect();
        step = board
            .answer_internal_card(question.actor, question.token, Answer::Targets(chosen))
            .unwrap();
    }
}

pub(super) fn settle_all(board: &mut Game) {
    loop {
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(q) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected a pending ability")
        };
        let step = board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        take_all(board, step);
    }
}

pub(super) fn attack(board: &mut Game, attacker: &str, target: &str) -> String {
    let outcome = board
        .internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
        .unwrap()
        .outcome;
    settle(board);
    while let Some(q) = board.internal_quick_choice().unwrap() {
        board
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        settle(board);
    }
    settle(board);
    outcome
}
