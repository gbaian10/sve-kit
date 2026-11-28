//! Scenario helpers shared by the English-only card batches.
use super::admission_bp_cp_fixture::{finish, settle};
use super::*;
use sve_engine::game::internal::SearchAnswer;

pub(super) fn state(board: &Game) -> Value {
    board.projection(View::Referee).unwrap()
}

/// Answers target and grouped search choices with as many candidates as allowed; other
/// questions use the fixture.
pub(super) fn take_all(board: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = &step {
        if let InputDomain::Search(domain) = &question.domain
            && let Some(groups) = &domain.groups
        {
            let mut used = Vec::new();
            let picks = groups
                .iter()
                .map(|group| {
                    let allowed = group
                        .cardinality
                        .maximum
                        .map_or(group.handles.len(), |maximum| {
                            usize::try_from(maximum.0).unwrap()
                        });
                    let pick = group
                        .handles
                        .iter()
                        .copied()
                        .filter(|handle| !used.contains(handle))
                        .take(allowed)
                        .collect::<Vec<_>>();
                    used.extend(&pick);
                    pick
                })
                .collect();
            step = board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Search(SearchAnswer::Grouped(picks)),
                )
                .unwrap();
            continue;
        }
        let InputDomain::Targets(domain) = &question.domain else {
            finish(board, step);
            return;
        };
        let allowed = domain
            .cardinality
            .maximum
            .map_or(domain.members.len(), |maximum| {
                usize::try_from(maximum.0).unwrap()
            });
        let chosen = domain
            .members
            .iter()
            .take(allowed)
            .map(|member| member.handle)
            .collect();
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
