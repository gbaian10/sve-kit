//! BP01-008's shape: the chosen follower's player puts it, or every opposing follower, at the
//! deck top or bottom, deciding each one and the order it is put there (Q156).
#![expect(
    clippy::panic,
    reason = "A missing placement question invalidates the scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{DecisionView, InputDomain, TargetRoleAssignment};

/// An activated ability with `n` formal opposing follower targets `victim` and body `steps`.
fn ability(n: u64, steps: &Value) -> Value {
    json!({"cost":{"pp":0},
        "targets":[{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":n}],
        "do":steps})
}

fn edges(objects: &Value, extra: &Value) -> Value {
    let mut movement = json!({"ref":objects,"to":"deck",
        "position":{"one_of":["top","bottom"],"by":{"master_of":"victim"}}});
    for (key, value) in extra.as_object().unwrap() {
        movement[key] = value.clone();
    }
    json!({"move":movement})
}

/// `seat` activates `body` against the other player's followers `t0..`, declaring the
/// `declared` handles as its targets.
fn start(
    seat: u32,
    body: &Value,
    followers: usize,
    declared: Vec<u64>,
) -> (Game, Result<CardPlay, EngineFailure>, &'static str) {
    let (me, them) = if seat == 1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    let programs: BTreeMap<String, WellFormedCardProgram> = lowerer()
        .lower(&evolution_linked_movement::tree(&[(
            "activate",
            body.clone(),
        )]))
        .unwrap();
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"][them]["zones"]["field"] = Value::Array(
        (0..followers)
            .map(|index| json!({"id":format!("t{index}"),"card":"unit-follower"}))
            .collect(),
    );
    initial["players"][them]["zones"]["deck"] =
        json!([{"id":"d0","card":"unit-spell"},{"id":"d1","card":"unit-spell"}]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "edges").unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let activated = game
        .activate_internal_card(
            "source",
            programs["unit-follower"].card().abilities[0].node.id,
            PlayerId { seat },
        )
        .unwrap();
    let CardPlay::Awaiting(target) = activated else {
        panic!("the formal target is declared");
    };
    let play = game.answer_internal_card(target.actor, target.token, Answer::Targets(declared));
    (game, play, them)
}

/// `seat` activates the plain shape against followers `t0..`; returns the placement question.
fn placement(seat: u32, objects: &Value, followers: usize) -> (Game, DecisionView, &'static str) {
    let body = ability(1, &json!([edges(objects, &json!({}))]));
    let (game, play, them) = start(seat, &body, followers, vec![0]);
    let play = play.unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("the victim's player places it");
    };
    (game, question, them)
}

fn members(question: &DecisionView) -> usize {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a placement question");
    };
    domain.members.len()
}

fn place(game: &mut Game, question: &DecisionView, top: &[u64], bottom: &[u64]) -> CardPlay {
    let targets = top.iter().chain(bottom).copied().collect();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::TargetsWithRoles {
            targets,
            roles: vec![
                TargetRoleAssignment {
                    role: Nat(1),
                    targets: top.to_vec(),
                },
                TargetRoleAssignment {
                    role: Nat(2),
                    targets: bottom.to_vec(),
                },
            ],
        },
    )
    .unwrap()
}

fn deck(game: &Game, player: &str) -> Vec<String> {
    game.projection(View::Referee).unwrap()[player]["deck"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_chosen_followers_player_puts_it_at_the_top_or_bottom() {
    for seat in [1_u32, 2] {
        for top in [true, false] {
            let (mut game, question, them) = placement(seat, &json!("victim"), 2);
            assert_eq!(
                question.actor,
                PlayerId {
                    seat: if seat == 1 { 2 } else { 1 }
                },
                "the victim's player decides"
            );
            assert_eq!(members(&question), 1, "only the chosen follower");
            let (up, down): (&[u64], &[u64]) = if top { (&[0], &[]) } else { (&[], &[0]) };
            let before = exact_authority(&game);
            game.answer_internal_card(
                PlayerId { seat },
                question.token,
                Answer::TargetsWithRoles {
                    targets: vec![0],
                    roles: vec![
                        TargetRoleAssignment {
                            role: Nat(1),
                            targets: vec![0],
                        },
                        TargetRoleAssignment {
                            role: Nat(2),
                            targets: vec![],
                        },
                    ],
                },
            )
            .unwrap_err();
            assert_eq!(
                exact_authority(&game),
                before,
                "only the victim's player answers"
            );
            resolved(place(&mut game, &question, up, down));
            let deck = deck(&game, them);
            assert_eq!(deck.len(), 3, "seat={seat} top={top}: {deck:?}");
            let edge = if top { deck.first() } else { deck.last() };
            assert!(
                edge.is_some_and(|id| id.starts_with('t')),
                "seat={seat} top={top}: {deck:?}"
            );
        }
    }
}

#[test]
fn every_opposing_follower_is_split_and_ordered_by_its_player() {
    // Q156: the player splits and orders both piles; each pile lists its cards top first.
    let (mut game, question, them) =
        placement(1, &json!({"who":"opp","zone":"field","kind":"follower"}), 4);
    assert_eq!(question.actor, PlayerId { seat: 2 });
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a placement question");
    };
    assert_eq!(domain.members.len(), 4, "every opposing follower");
    assert_eq!(
        (domain.cardinality.minimum, domain.cardinality.maximum),
        (Nat(4), Some(Nat(4))),
        "every follower is placed"
    );
    let before = exact_authority(&game);
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::TargetsWithRoles {
            targets: vec![0, 1, 2],
            roles: vec![
                TargetRoleAssignment {
                    role: Nat(1),
                    targets: vec![0, 2],
                },
                TargetRoleAssignment {
                    role: Nat(2),
                    targets: vec![1],
                },
            ],
        },
    )
    .unwrap_err();
    assert_eq!(
        exact_authority(&game),
        before,
        "a follower cannot be left out"
    );
    resolved(place(&mut game, &question, &[2, 0], &[3, 1]));
    // Candidates follow field order, so handle n is t{n}.
    assert_eq!(deck(&game, them), ["t2", "t0", "d0", "d1", "t3", "t1"]);
}

#[test]
fn a_target_that_left_the_field_is_no_longer_placed() {
    // The target is destroyed first: nothing is left to place, and it stays in the cemetery.
    let body = ability(
        1,
        &json!([{"destroy":"victim"}, edges(&json!("victim"), &json!({}))]),
    );
    let (mut game, play, them) = start(1, &body, 2, vec![0]);
    let play = play.unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("the empty placement is still the victim's player's decision");
    };
    assert_eq!(members(&question), 0);
    resolved(place(&mut game, &question, &[], &[]));
    assert_eq!(deck(&game, them), ["d0", "d1"]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state[them]["cemetery"].as_array().unwrap().len(), 1);

    // Of two targets one is destroyed first; only the other is placed.
    let body = ability(
        2,
        &json!([
            {"choose":{"bind":"doomed","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}},
            {"destroy":"doomed"},
            // `master_of` names one object's player; both targets share the opponent here.
            {"move":{"ref":"victim","to":"deck","position":{"one_of":["top","bottom"],"by":"opp"}}}
        ]),
    );
    let (mut game, play, them) = start(1, &body, 3, vec![0, 1]);
    let play = play.unwrap();
    let CardPlay::Awaiting(doomed) = play else {
        panic!("the effect controller chooses one to destroy");
    };
    let InputDomain::Targets(domain) = &doomed.domain else {
        panic!("a target question");
    };
    assert_eq!(domain.members.len(), 3);
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(doomed.actor, doomed.token, Answer::Targets(vec![0]))
        .unwrap()
    else {
        panic!("the remaining target is placed");
    };
    assert_eq!(members(&question), 1, "only the target still on the field");
    resolved(place(&mut game, &question, &[0], &[]));
    assert_eq!(deck(&game, them), ["t1", "d0", "d1"]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state[them]["cemetery"], json!(["t0"]));
    assert_eq!(state[them]["field"], json!(["t2"]));
}

#[test]
fn naming_the_effect_controller_as_the_mover_keeps_the_same_placement() {
    // `move.by` names who puts the objects; `position.by` still decides where they go.
    for extra in [json!({}), json!({"by":"self"})] {
        let body = ability(1, &json!([edges(&json!("victim"), &extra)]));
        let (mut game, play, them) = start(1, &body, 1, vec![0]);
        let play = play.unwrap();
        let CardPlay::Awaiting(question) = play else {
            panic!("the victim's player decides: {extra}");
        };
        assert_eq!(question.actor, PlayerId { seat: 2 }, "{extra}");
        resolved(place(&mut game, &question, &[], &[0]));
        assert_eq!(deck(&game, them), ["d0", "d1", "t0"], "{extra}");
    }
}

#[test]
fn an_ordinary_multi_card_deck_move_still_needs_an_order_choice() {
    // Only a deck-edge pile was ordered by its answer; a plain query of two cards was not.
    let body = ability(
        1,
        &json!([{"move":{"ref":{"who":"opp","zone":"field","kind":"follower"},"to":"deck","position":"top"}}]),
    );
    let (_, play, _) = start(1, &body, 2, vec![0]);
    assert!(
        matches!(&play, Err(EngineFailure::Unsupported(reason)) if reason.contains("order choice")),
        "{play:?}"
    );
}
