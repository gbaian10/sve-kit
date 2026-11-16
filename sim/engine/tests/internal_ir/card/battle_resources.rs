use super::*;
use crate::{board_fixture, board_fixtures};
use sve_engine::battle::{BattleReferences, ProjectionError};
use sve_engine::game::player::{
    Command, EvolutionAnswer, EvolutionQuestion, Input, OpeningAnswer, OpeningQuestion,
    QuestionShape, SubmissionKind, Viewer,
};

fn position(seat: u32) -> Value {
    let mut value = board_fixtures::setup(seat, json!([]));
    value["players"]["P1"]["ep_capacity"] = json!(0_i32);
    value["players"]["P1"]["sep_available_from"] = Value::Null;
    value["players"]["P2"]["ep"] = json!(1_i32);
    value["players"]["P2"]["ep_capacity"] = json!(3_i32);
    value["players"]["P2"]["sep"] = json!(1_i32);
    value["players"]["P2"]["sep_available_from"] = json!(6_u32);
    value
}

fn game(initial: &Value) -> Game {
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-resources",
    )
    .unwrap();
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn players(game: &Game, viewer: Viewer) -> Value {
    let before = exact_authority(game);
    let board = game
        .battle_observation(viewer, &board_fixture::fixture().unwrap().references)
        .unwrap();
    assert!(
        exact_authority(game) == before,
        "resource projection cannot mutate the authority"
    );
    serde_json::to_value(board.players).unwrap()
}

#[test]
fn wire_evolution_resources_keep_spent_ep_and_absent_sep_explicit() {
    let game = game(&position(1));
    for viewer in [Viewer::P1, Viewer::P2] {
        let players = players(&game, viewer);
        assert_eq!(players[0]["ep_capacity"], 0_i32);
        assert_eq!(players[0]["ep_used"], 0_i32);
        assert_eq!(players[0]["sep_available_from"], Value::Null);
        assert_eq!(players[1]["ep"], 1_i32);
        assert_eq!(players[1]["ep_capacity"], 3_i32);
        assert_eq!(players[1]["ep_used"], 2_i32);
        assert_eq!(players[1]["sep_available_from"], 6_u32);
    }
}

fn assert_ep(game: &Game, seat: u32, expected: (i64, i64, i64)) {
    let index = usize::from(seat != 1);
    for viewer in [Viewer::P1, Viewer::P2] {
        let state = players(game, viewer);
        assert_eq!(
            (
                state[index]["ep"].as_i64(),
                state[index]["ep_capacity"].as_i64(),
                state[index]["ep_used"].as_i64()
            ),
            (Some(expected.0), Some(expected.1), Some(expected.2)),
            "both viewers receive the same authoritative EP pool"
        );
    }
}

fn increase(game: &mut Game, seat: u32, amount: i64) -> sve_engine::Result<Progress> {
    increases(game, seat, &[amount])
}

fn increases(game: &mut Game, seat: u32, amounts: &[i64]) -> sve_engine::Result<Progress> {
    let steps = amounts
        .iter()
        .enumerate()
        .map(|(index, amount)| {
            instruction(
                u64::try_from(index).unwrap().checked_add(1).unwrap(),
                Action::Resource {
                    player: PlayerRef::Controller {},
                    resource: Resource::Ep,
                    operation: ResourceOperation::Add,
                    amount: Expr::Literal(Int(*amount)),
                },
            )
        })
        .collect();
    game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
}

#[test]
fn wire_evolution_resources_recover_slots_then_expand_for_larger_ep_pools() {
    for seat in [1_u32, 2_u32] {
        let mut initial = position(seat);
        let actor = if seat == 1 { "P1" } else { "P2" };
        initial["players"][actor]["ep"] = json!(1_i32);
        initial["players"][actor]["ep_capacity"] = json!(3_i32);
        let mut game = game(&initial);
        for (amount, expected) in [
            (1_i64, (2_i64, 3_i64, 1_i64)),
            (1, (3, 3, 0)),
            (2, (5, 5, 0)),
        ] {
            increase(&mut game, seat, amount).unwrap();
            assert_ep(&game, seat, expected);
        }
        let before = exact_authority(&game);
        assert!(matches!(
            increase(&mut game, seat, i64::MAX),
            Err(sve_engine::EngineFailure::Invariant(_))
        ));
        assert!(
            exact_authority(&game) == before,
            "an overflowing increase cannot expand the retained capacity"
        );
        assert!(matches!(
            increases(&mut game, seat, &[2_i64, i64::MAX]),
            Err(sve_engine::EngineFailure::Invariant(_))
        ));
        assert!(
            exact_authority(&game) == before,
            "a later failure restores EP capacity grown earlier in the same transaction"
        );
        assert_ep(&game, seat, (5, 5, 0));
    }
}

fn payment(actor: Viewer, elapsed: u64, unlock: Option<u64>) -> (Game, BattleReferences) {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let own = if actor == Viewer::P1 { "P1" } else { "P2" };
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["turn"]["elapsed_turns"][own] = json!(elapsed);
    initial["players"][own]["ep"] = json!(1_i32);
    initial["players"][own]["ep_capacity"] = json!(3_i32);
    initial["players"][own]["sep"] = json!(1_i32);
    if let Some(turn) = unlock {
        initial["players"][own]["sep_available_from"] = json!(turn);
    }
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"][own]["zones"]["evolve_deck"] = json!([{"id":"e","card":"unit-form"}]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "resource-payment",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &evolution::programs().0)
        .unwrap();
    for number in ["unit-form", "unit-spell", "unit-amulet", "unit-other"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    player_evolution::start(&mut game, actor);
    player_evolution::form(&mut game, actor);
    (game, references)
}

fn assert_sep(
    game: &Game,
    references: &BattleReferences,
    actor: Viewer,
    stock: i64,
    turn: Option<u64>,
) {
    for viewer in [Viewer::P1, Viewer::P2] {
        let board = game.battle_observation(viewer, references).unwrap();
        let player = board
            .players
            .iter()
            .find(|p| {
                p.seat
                    == if actor == Viewer::P1 {
                        sve_battle_contract::Seat::P1
                    } else {
                        sve_battle_contract::Seat::P2
                    }
            })
            .unwrap();
        assert_eq!(
            player.sep.get(),
            stock,
            "SEP stock comes from the player state"
        );
        assert_eq!(
            player.sep_available_from.map(sve_battle_contract::Nat::get),
            turn,
            "the allocation schedule persists independently of the remaining stock"
        );
    }
}

#[test]
fn wire_evolution_resources_keep_sep_unlock_after_ep_and_sep_payment() {
    for (actor, turn) in [(Viewer::P1, 7_u64), (Viewer::P2, 6_u64)] {
        for elapsed in [turn.checked_sub(1).unwrap(), turn] {
            let (mut game, references) = payment(actor, elapsed, None);
            let native = player_pipeline::question(&game, actor);
            let QuestionShape::Evolution(EvolutionQuestion::Payment(quote)) = native.shape else {
                panic!("actual evolution payment")
            };
            assert_eq!(quote.super_allowed, elapsed >= turn);
            assert_sep(&game, &references, actor, 1, Some(turn));
            let command = Command::Evolution {
                token: native.token,
                answer: EvolutionAnswer::Payment {
                    pp: quote.total_pp.checked_sub(1).unwrap(),
                    ep: true,
                    super_evolution: true,
                },
            };
            let before = exact_authority(&game);
            let result = game.submit_player(actor, command);
            if elapsed < turn {
                assert_eq!(result.kind, SubmissionKind::Rejected);
                assert!(
                    exact_authority(&game) == before,
                    "locked SEP rejection preserves resources and capacity"
                );
            } else {
                assert_eq!(result.kind, SubmissionKind::Applied);
                for viewer in [Viewer::P1, Viewer::P2] {
                    let board = game.battle_observation(viewer, &references).unwrap();
                    let index = usize::from(actor != Viewer::P1);
                    assert_eq!(
                        (
                            board.players[index].ep.get(),
                            board.players[index].ep_capacity.get(),
                            board.players[index].ep_used.get()
                        ),
                        (0_i64, 3_u64, 3_u64)
                    );
                }
                assert_sep(&game, &references, actor, 0, Some(turn));
            }
        }
    }
}

#[test]
fn wire_evolution_resources_use_the_state_schedule_for_sep_legality() {
    for (elapsed, turn, allowed) in [(6_u64, 9_u64, false), (9, 9, true), (7, 4, true)] {
        let (game, references) = payment(Viewer::P2, elapsed, Some(turn));
        let native = player_pipeline::question(&game, Viewer::P2);
        let QuestionShape::Evolution(EvolutionQuestion::Payment(quote)) = native.shape else {
            panic!("actual evolution payment")
        };
        assert_eq!(quote.super_allowed, allowed);
        assert_sep(&game, &references, Viewer::P2, 1, Some(turn));
    }
}

#[test]
fn wire_evolution_resources_refuse_invalid_capacity_and_unlock_numbers() {
    for (field, value) in [
        ("ep_capacity", json!(-1_i32)),
        ("ep_capacity", json!(0_i32)),
        (
            "ep_capacity",
            json!(
                sve_battle_contract::MAX_SAFE_INTEGER
                    .checked_add(1)
                    .unwrap()
            ),
        ),
        (
            "sep_available_from",
            json!(
                sve_battle_contract::MAX_SAFE_INTEGER
                    .checked_add(1)
                    .unwrap()
            ),
        ),
    ] {
        let mut initial = position(1);
        initial["players"]["P2"][field] = value;
        let game = game(&initial);
        assert!(matches!(
            game.battle_observation(Viewer::P1, &board_fixture::fixture().unwrap().references),
            Err(ProjectionError::Range)
        ));
    }
    let mut initial = position(1);
    initial["players"]["P2"]["sep_available_from"] = Value::Null;
    assert!(matches!(
        game(&initial)
            .battle_observation(Viewer::P1, &board_fixture::fixture().unwrap().references),
        Err(ProjectionError::Unavailable)
    ));
}

fn finish_opening(game: &mut Game, first: Viewer) -> Option<()> {
    for _step in 0_u32..3_u32 {
        let native = [Viewer::P1, Viewer::P2].into_iter().find_map(|viewer| {
            if let Input::Question(question) = game.input(viewer) {
                Some(question)
            } else {
                None
            }
        })?;
        let QuestionShape::Opening(shape) = native.shape else {
            return None;
        };
        let answer = match shape {
            OpeningQuestion::FirstPlayer => OpeningAnswer::FirstPlayer(first),
            OpeningQuestion::Mulligan(_) => OpeningAnswer::Keep,
            OpeningQuestion::StartAmulet(_) => return None,
        };
        assert_eq!(
            game.submit_player(
                native.actor,
                Command::Opening {
                    token: native.token,
                    answer
                }
            )
            .kind,
            SubmissionKind::Applied,
            "each real opening question advances to the next stage"
        );
    }
    Some(())
}

#[test]
fn wire_evolution_resources_follow_real_opening_allocations_for_either_first_player() {
    for first in [Viewer::P1, Viewer::P2] {
        let mut initial = json!({"pregame":true,"players":{}});
        for seat in ["P1", "P2"] {
            initial["players"][seat] = json!({"construction":"class","leader":{"class":"ニュートラル"},"deck_list":(0_u32..8_u32).map(|index|json!({"id":format!("{seat}-deck-{index}"),"card":"unit-spell"})).collect::<Vec<_>>(),"evolve_deck_list":[]});
        }
        let mut game = game(&initial);
        for viewer in [Viewer::P1, Viewer::P2] {
            let before = players(&game, viewer);
            for index in [0_usize, 1_usize] {
                assert_eq!(before[index]["ep_capacity"], 0_i32);
                assert_eq!(before[index]["sep_available_from"], Value::Null);
            }
        }
        finish_opening(&mut game, first).unwrap();
        assert!(
            matches!(game.input(first),Input::Question(question) if matches!(question.shape,QuestionShape::ActionWindow(_)))
        );
        for viewer in [Viewer::P1, Viewer::P2] {
            let after = players(&game, viewer);
            let first_index = usize::from(first != Viewer::P1);
            let other_index = usize::from(first == Viewer::P1);
            assert_eq!(after[first_index]["ep_capacity"], 0_i32);
            assert_eq!(after[other_index]["ep_capacity"], 3_i32);
            assert_eq!(after[first_index]["ep_used"], 0_i32);
            assert_eq!(after[other_index]["ep_used"], 0_i32);
            assert_eq!(after[first_index]["sep_available_from"], 7_u32);
            assert_eq!(after[other_index]["sep_available_from"], 6_u32);
        }
    }
}
