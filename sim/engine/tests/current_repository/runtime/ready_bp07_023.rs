#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed card scenario."
)]
use super::filter_blocks::{build, checks, finish, position};
use super::*;

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn posture(game: &Game, standing: bool) {
    let current = state(game);
    assert_eq!(
        current["objects"]["source"]["acted"], !standing,
        "the holder has the expected posture"
    );
    for keyword in ["威圧", "オーラ"] {
        assert_eq!(
            current["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!(keyword)),
            standing,
            "{keyword} follows the evolved field holder's posture"
        );
    }
}

fn single_trigger(game: &mut Game, card: &str) -> CardPlay {
    checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(
        options.len(),
        1,
        "the base and evolved abilities must not both trigger"
    );
    assert_eq!(
        options[0].card, card,
        "the evolved face provides the ability"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn pass_quick(game: &mut Game) {
    for _ in 0..4_u8 {
        let Some(question) = game.internal_quick_choice().unwrap() else {
            return;
        };
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, play, &[]);
    }
    panic!("Quick window did not close")
}

fn end_discard(game: &mut Game) {
    pass_quick(game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    checks(game);
}

fn resolve_evolution(game: &mut Game, card: &str, leader: bool) {
    let before = state(game)["P1"]["pp"]["current"].as_u64().unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed action resolves with available resources"
    );
    let play = single_trigger(game, card);
    let CardPlay::Awaiting(question) = &play else {
        panic!("evolution targets")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("leader or follower")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the enemy follower and its leader qualify"
    );
    let chosen = domain
        .members
        .iter()
        .find(|member| member.card.is_none() == leader)
        .unwrap();
    let answer = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![chosen.handle]),
        )
        .unwrap();
    finish(game, answer, &[]);
    checks(game);
    let current = state(game);
    assert_eq!(
        current["P1"]["pp"]["current"],
        before.checked_sub(2).unwrap(),
        "evolution spends exactly two PP"
    );
    assert_eq!(
        current["objects"]["source"]["power"], 2_u8,
        "evolution installs printed power"
    );
    assert_eq!(
        current["objects"]["source"]["hp"], 4_u8,
        "evolution installs printed health"
    );
    assert_eq!(
        current["objects"]["enemy"]["damage"],
        if leader { 0_u8 } else { 2_u8 },
        "evolution damages only the chosen recipient"
    );
    assert_eq!(
        current["P2"]["leader"]["life"],
        if leader { 18_u8 } else { 20_u8 },
        "the enemy leader takes only chosen evolution damage"
    );
    posture(game, true);
}

fn attack_and_end(game: &mut Game, card: &str, leader: bool) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed action resolves with available resources"
    );
    pass_quick(game);
    posture(game, false);
    assert_eq!(
        state(game)["P2"]["leader"]["life"],
        if leader { 16_u8 } else { 18_u8 },
        "the attack deals the evolved holder power"
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let play = single_trigger(game, card);
    assert_eq!(
        filter_blocks::offered(&play),
        ["flt-enemy"],
        "end damage cannot choose a leader or an allied follower"
    );
    finish(game, play, &[]);
    checks(game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "one evolved end trigger resolves once"
    );
    assert_eq!(
        state(game)["objects"]["enemy"]["damage"],
        if leader { 1_u8 } else { 3_u8 },
        "only the own end trigger adds one damage"
    );
    end_discard(game);
    assert_eq!(
        state(game)["turn"]["active"],
        "P2",
        "own end yields to the opponent"
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    checks(game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "opponent end does not trigger this ability"
    );
    assert_eq!(
        state(game)["objects"]["enemy"]["damage"],
        if leader { 1_u8 } else { 3_u8 },
        "only the own end trigger adds one damage"
    );
    end_discard(game);
    assert_eq!(
        state(game)["turn"]["active"],
        "P1",
        "opponent end yields a fresh own turn"
    );
    posture(game, true);
}

#[test]
fn bp07_023_real_evolution_targets_both_kinds_and_tracks_posture_and_own_end() {
    for card in ["BP07-023", "BP21-PR06", "PR-216", "PR-326"] {
        for leader in [false, true] {
            let mut initial = position("BP07-022", "field");
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ally","card":"flt-other"}));
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"flt-enemy"},{"id":"charm","card":"flt-amulet"}]);
            let mut game = build(&initial);
            resolve_evolution(&mut game, card, leader);
            attack_and_end(&mut game, card, leader);
        }
    }
}
