#![expect(
    clippy::panic,
    reason = "A missing private discard question invalidates the fixture."
)]
use super::*;
use sve_engine::game::internal::{DecisionToken, DecisionView, InputDomain};
use sve_engine::game::player::{Input, Viewer};

fn input() -> Value {
    evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"each_player":{"as":"p","stages":[[{"discard":{"who":"p","down_to":2}}]]}}
        ]}),
    )])
}

const fn viewer(seat: u32) -> Viewer {
    if seat == 1 { Viewer::P1 } else { Viewer::P2 }
}

fn position(seat: u32, own: usize, opposing: usize, program: &WellFormedCardProgram) -> Game {
    let (me, them) = if seat == 1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    for (player, n, prefix) in [(me, own, "mine"), (them, opposing, "theirs")] {
        initial["players"][player]["zones"]["hand"] = json!(
            (0..n)
                .map(|i| json!({"id":format!("{prefix}{i}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = evolution_linked_movement::loaded(&initial, program);
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn answer(
    game: &mut Game,
    q: &DecisionView,
    count: usize,
    previous: Option<DecisionToken>,
) -> CardPlay {
    let other = if q.actor.seat == 1 { 2 } else { 1 };
    assert!(
        matches!(game.input(viewer(q.actor.seat)), Input::Question(_)),
        "only the holder receives the question"
    );
    assert!(
        matches!(game.input(viewer(other)), Input::Waiting(_)),
        "the other player sees a wait"
    );
    let (own_view, opposing) = if q.actor.seat == 1 {
        (View::P1, "P2")
    } else {
        (View::P2, "P1")
    };
    let projection = game.projection(own_view).unwrap().to_string();
    for id in game.projection(View::Referee).unwrap()[opposing]["hand"]
        .as_array()
        .unwrap()
    {
        assert!(
            !projection.contains(&id.to_string()),
            "the opponent's retained hand identities stay hidden"
        );
    }
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private discard")
    };
    assert_eq!(
        domain.members.len(),
        count,
        "only the holder's hand is offered"
    );
    let excess = count.checked_sub(2).unwrap();
    let amount = Nat(u64::try_from(excess).unwrap());
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: amount,
            maximum: Some(amount)
        },
        "discard exactly the excess"
    );
    assert!(
        domain.members.iter().all(|member| member.player == q.actor),
        "all candidates belong to the holder"
    );
    let before = exact_authority(game);
    game.answer_internal_card(PlayerId { seat: other }, q.token, Answer::Targets(vec![]))
        .unwrap_err();
    if let Some(old) = previous {
        game.answer_internal_card(q.actor, old, Answer::Targets(vec![]))
            .unwrap_err();
    }
    assert_eq!(
        exact_authority(game),
        before,
        "refused answers retain all authority"
    );
    let handles = domain
        .members
        .iter()
        .rev()
        .take(excess)
        .map(|member| member.handle)
        .collect();
    game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
        .unwrap()
}

#[test]
fn each_player_discards_their_own_hand_down_to_two_in_turn_order_without_private_leaks() {
    let program = lowerer()
        .lower(&input())
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for seat in [1, 2] {
        let (me, them, other) = if seat == 1 {
            ("P1", "P2", 2)
        } else {
            ("P2", "P1", 1)
        };
        for (own, opposing) in [(0, 5), (1, 3), (2, 2), (5, 1), (4, 5)] {
            let mut game = position(seat, own, opposing, &program);
            let mut play = game
                .activate_internal_card("s", program.card().abilities[0].node.id, PlayerId { seat })
                .unwrap();
            let mut asked = vec![];
            let mut previous = None;
            while let CardPlay::Awaiting(q) = play {
                let count = if q.actor.seat == seat { own } else { opposing };
                if previous.is_some() {
                    assert_eq!(
                        game.projection(View::Referee).unwrap()[me]["cemetery"]
                            .as_array()
                            .unwrap()
                            .len(),
                        own - 2
                    );
                }
                asked.push(q.actor.seat);
                play = answer(&mut game, &q, count, previous);
                previous = Some(q.token);
            }
            resolved(play);
            let expected = [(seat, own), (other, opposing)]
                .into_iter()
                .filter_map(|(player, count)| (count > 2).then_some(player))
                .collect::<Vec<_>>();
            assert_eq!(asked, expected);
            let state = game.projection(View::Referee).unwrap();
            for (player, count) in [(me, own), (them, opposing)] {
                assert_eq!(
                    state[player]["hand"].as_array().unwrap().len(),
                    count.min(2)
                );
                assert_eq!(
                    state[player]["cemetery"].as_array().unwrap().len(),
                    count.saturating_sub(2)
                );
            }
        }
    }
}
