#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the distribution scenarios."
)]
use super::dynamic_declarations::lower_body;
use super::*;
use sve_engine::game::internal::{DamageShare, DecisionView, InputDomain};

fn body(draw_first: bool) -> Value {
    let damage = json!({"damage":{"to":"enemies","n":{"plus":[
        {"count":{"who":"self","zone":"hand"}},
        {"count":{"who":"self","zone":"ex"}}
    ]},"distribute":true}});
    let steps = if draw_first {
        json!([{"draw":{"n":1}}, damage])
    } else {
        json!([damage])
    };
    json!({"targets":[{"bind":"enemies","from":{
        "who":"opp","zone":"field","kind":"follower"},"any":true}],"do":steps})
}

fn position(draw_first: bool, total: usize) -> Game {
    let program = lower_body(&body(draw_first)).unwrap();
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-spell"}]));
    let mut hand = vec![json!({"id":"source","card":"unit-follower"})];
    hand.extend(
        (0..total.saturating_sub(1)).map(|i| json!({"id":format!("hand-{i}"),"card":"unit-spell"})),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!(hand);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = if total == 0 {
        json!([])
    } else {
        json!([{"id":"extra","card":"unit-spell"}])
    };
    initial["players"]["P2"]["zones"]["field"] = json!(
        (0..4)
            .map(|i| json!({"id":format!("enemy-{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "hand-distribution",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn entered(draw_first: bool, total: usize) -> (Game, DecisionView) {
    let mut game = position(draw_first, total);
    game.play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let CardPlay::Awaiting(targets) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("formal targets")
    };
    (game, targets)
}

#[test]
fn zero_targets_skip_distribution_but_still_execute_the_body_at_zero_and_positive_totals() {
    for total in [0, 3] {
        let (mut game, targets) = entered(true, total);
        let result = resolved(choose(&mut game, &targets, 0));
        assert!(result.progress.complete);
        assert!(game.internal_decision(View::Referee).is_none());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), total.max(1));
        for i in 0..4 {
            assert_eq!(state["objects"][format!("enemy-{i}")]["damage"], 0);
        }
    }
}

#[test]
fn too_many_recipients_retry_targets_without_executing_the_body() {
    for total in [0, 3] {
        let (mut game, targets) = entered(true, total);
        let CardPlay::Awaiting(retry) = choose(&mut game, &targets, total + 1) else {
            panic!("infeasible selection retries targets")
        };
        assert!(matches!(retry.domain, InputDomain::Targets(_)));
        assert_ne!(retry.token, targets.token);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"].as_array().unwrap().len(),
            total.saturating_sub(1)
        );
        assert_eq!(state["P1"]["deck"], json!(["drawn"]));
        resolved(choose(&mut game, &retry, 0));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["deck"],
            json!([])
        );
    }
}

fn shares(values: &[(u64, u64)]) -> Answer {
    Answer::Distribution(
        values
            .iter()
            .map(|&(handle, amount)| DamageShare {
                handle,
                amount: Nat(amount),
            })
            .collect(),
    )
}

#[test]
fn invalid_distribution_answers_preserve_the_whole_authority_and_all_views() {
    let (mut game, targets) = entered(false, 3);
    let CardPlay::Awaiting(question) = choose(&mut game, &targets, 2) else {
        panic!("distribution")
    };
    let InputDomain::Distribution(domain) = &question.domain else {
        panic!("distribution")
    };
    let [a, b] = domain.members.iter().map(|m| m.handle).collect::<Vec<_>>()[..] else {
        panic!("two recipients")
    };
    for invalid in [
        vec![(a, 2), (b, 2)],
        vec![(a, 3), (b, 0)],
        vec![(a, 3)],
        vec![(a, 1), (b, 1), (u64::MAX, 1)],
    ] {
        let before = exact_authority(&game);
        let views = [View::P1, View::P2, View::Referee];
        let observations = views.map(|v| game.projection(v).unwrap());
        game.answer_internal_card(question.actor, question.token, shares(&invalid))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        assert_eq!(views.map(|v| game.projection(v).unwrap()), observations);
        assert_eq!(
            game.internal_decision(View::Referee),
            Some(question.clone())
        );
    }
    resolved(
        game.answer_internal_card(question.actor, question.token, shares(&[(a, 2), (b, 1)]))
            .unwrap(),
    );
}

#[test]
fn player_commands_complete_the_fanfare_and_freeze_its_distribution_before_the_body() {
    use crate::player_pipeline::{answer, question};
    use sve_engine::game::player::{DecisionShape, QuestionShape, SubmissionKind, Viewer};
    let mut game = position(true, 3);
    game.advance_player_input();
    let command = player_declarations::play_command(&game, "unit-follower");
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied
    );
    let pending = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Pending(options)) = &pending.shape else {
        panic!("pending fanfare")
    };
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Pending(options[0].handle)).kind,
        SubmissionKind::Applied
    );
    let targets = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Targets(domain)) = &targets.shape else {
        panic!("public targets")
    };
    let selected = domain.members.iter().take(2).map(|m| m.handle).collect();
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(selected)).kind,
        SubmissionKind::Applied
    );
    let distribution = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Distribution(domain)) = &distribution.shape else {
        panic!("public distribution")
    };
    assert_eq!(domain.total, Nat(3));
    let values = domain
        .members
        .iter()
        .zip([2, 1])
        .map(|(m, n)| (m.handle, n))
        .collect::<Vec<_>>();
    assert_eq!(
        answer(&mut game, Viewer::P1, shares(&values)).kind,
        SubmissionKind::Applied
    );
    assert!(matches!(
        question(&game, Viewer::P1).shape,
        QuestionShape::ActionWindow(_)
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
    assert_eq!(state["objects"]["enemy-0"]["damage"], 2);
    assert_eq!(state["objects"]["enemy-1"]["damage"], 1);
}

fn choose(game: &mut Game, targets: &DecisionView, count: usize) -> CardPlay {
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("targets")
    };
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(0),
            maximum: None
        },
        "any target count has no printed numeric cap"
    );
    game.answer_internal_card(
        targets.actor,
        targets.token,
        Answer::Targets(
            domain
                .members
                .iter()
                .take(count)
                .map(|m| m.handle)
                .collect(),
        ),
    )
    .unwrap()
}

#[test]
fn fanfare_hand_and_ex_distribution_uses_the_total_before_the_body_draws() {
    let (mut game, targets) = entered(true, 3);
    let CardPlay::Awaiting(question) = choose(&mut game, &targets, 2) else {
        panic!("distribution")
    };
    let InputDomain::Distribution(domain) = &question.domain else {
        panic!("distribution")
    };
    assert_eq!(domain.total, Nat(3));
    assert_eq!(domain.minimum, Nat(1));
    assert_eq!(domain.members.len(), 2);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Distribution(
                domain
                    .members
                    .iter()
                    .zip([2, 1])
                    .map(|(member, n)| DamageShare {
                        handle: member.handle,
                        amount: Nat(n),
                    })
                    .collect(),
            ),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    assert_eq!(state["objects"]["enemy-0"]["damage"], 2);
    assert_eq!(state["objects"]["enemy-1"]["damage"], 1);
}
