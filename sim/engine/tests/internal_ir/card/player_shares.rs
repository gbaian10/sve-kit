use super::*;

fn tree(steps: &Value) -> Value {
    equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0_u8},"do":steps}}),
        "unit-amulet",
    )
}

fn procedure(stages: &Value) -> Value {
    json!({"each_player":{"as":"p","stages":stages}})
}

fn return_hand() -> Value {
    json!([
        {"move":{"ref":{"who":"p","zone":"hand"},"to":"deck","result":"returned"}},
        {"shuffle":{"who":"p"}}
    ])
}

fn draw_back() -> Value {
    json!({"draw":{"who":"p","n":{"count_of":"returned"}}})
}

/// BP01-161's shape: each player returns their whole hand, shuffles and draws as many as
/// they returned; the controller then draws one more.
fn new_destiny() -> Value {
    json!([procedure(&json!([return_hand(), [draw_back()]])), {"draw":{"n":1_u8}}])
}

fn program(steps: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(steps))
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn cards(prefix: &str, count: usize) -> Value {
    Value::Array(
        (0..count)
            .map(|index| json!({"id":format!("{prefix}{index}"),"card":"unit-follower"}))
            .collect(),
    )
}

/// Runs `steps` for `seat` with the given (hand, deck) sizes per player; returns the draw facts.
fn run(
    steps: &Value,
    seat: u32,
    mine: (usize, usize),
    theirs: (usize, usize),
) -> (Game, Vec<(u32, u64)>) {
    let program = program(steps);
    let ability = program.card().abilities[0].node.id;
    let (me, them) = if seat == 1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    for (player, (held, deck)) in [(me, mine), (them, theirs)] {
        initial["players"][player]["zones"]["hand"] = cards(&format!("{player}h"), held);
        initial["players"][player]["zones"]["deck"] = cards(&format!("{player}d"), deck);
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "new-destiny",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let result = resolved(
        game.activate_internal_card("source", ability, PlayerId { seat })
            .unwrap(),
    );
    let draws = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Draw(draw) = fact {
                Some((draw.player.seat, draw.ordinal.0))
            } else {
                None
            }
        })
        .collect();
    (game, draws)
}

fn sizes(game: &Game, player: &str) -> (usize, usize) {
    let view = game.projection(View::Referee).unwrap();
    (
        view[player]["hand"].as_array().unwrap().len(),
        view[player]["deck"].as_array().unwrap().len(),
    )
}

#[test]
fn each_player_draws_their_own_returned_count() {
    // (my hand, their hand) -> each draws back their own count, Q240 allows an empty hand.
    for seat in [1_u32, 2] {
        let (me, them) = if seat == 1 {
            ("P1", "P2")
        } else {
            ("P2", "P1")
        };
        let other = if seat == 1 { 2 } else { 1 };
        for (mine, theirs) in [(3_usize, 1_usize), (0, 2)] {
            let (game, draws) = run(&new_destiny(), seat, (mine, 6), (theirs, 6));
            assert_eq!(sizes(&game, me), (mine + 1, 5), "seat={seat} mine={mine}");
            assert_eq!(
                sizes(&game, them),
                (theirs, 6),
                "seat={seat} theirs={theirs}"
            );
            if mine == 3 {
                // Both draw each ordinal in turn order until their own count is reached.
                assert_eq!(
                    draws,
                    [(seat, 1), (other, 1), (seat, 2), (seat, 3), (seat, 1)],
                    "seat={seat}"
                );
            }
        }
    }
}

#[test]
fn a_finished_return_may_be_read_later_in_the_same_stage() {
    let mut stage = return_hand().as_array().unwrap().clone();
    stage.push(draw_back());
    let (game, _) = run(&json!([procedure(&json!([stage]))]), 1, (3, 6), (1, 6));
    assert_eq!(sizes(&game, "P1"), (3, 6));
    assert_eq!(sizes(&game, "P2"), (1, 6));
}

#[test]
fn independent_procedures_may_reuse_a_result_name() {
    let once = procedure(&json!([return_hand(), [draw_back()]]));
    let (game, draws) = run(&json!([once, once]), 1, (3, 6), (1, 6));
    assert_eq!(sizes(&game, "P1"), (3, 6));
    assert_eq!(sizes(&game, "P2"), (1, 6));
    assert_eq!(draws.len(), 8, "each procedure draws back 3 + 1");
}

#[test]
fn a_short_deck_stops_only_its_own_players_draws() {
    // P1 returns three into an empty deck and P2 one into two; reading the count twice
    // empties P1's deck while P2 still draws its own card.
    let (game, draws) = run(
        &json!([procedure(&json!([
            return_hand(),
            [draw_back()],
            [draw_back()]
        ]))]),
        1,
        (3, 0),
        (1, 2),
    );
    assert_eq!(sizes(&game, "P1"), (3, 0));
    assert_eq!(sizes(&game, "P2"), (2, 1));
    assert_eq!(draws.last(), Some(&(2, 1)));
}

#[test]
fn a_returned_count_is_read_only_after_and_inside_its_own_procedure() {
    for steps in [
        // Read before the return exists.
        json!([procedure(&json!([[draw_back()], return_hand()]))]),
        // Read after the procedure ended.
        json!([
            procedure(&json!([return_hand()])),
            {"draw":{"n":{"count_of":"returned"}}}
        ]),
    ] {
        lowerer().lower(&tree(&steps)).unwrap_err();
    }
}
