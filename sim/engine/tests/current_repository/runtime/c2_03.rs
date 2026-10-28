#![expect(
    clippy::panic,
    reason = "Malformed typed questions fail this test scenario."
)]
use super::*;
use sve_engine::game::internal::DamageShare;
fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: seat_number(seat),
    }
}
fn play(game: &mut Game, seat: &str, quick: bool) -> CardPlay {
    if !quick {
        return game.play_internal_card("spell", actor(seat)).unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(
        question.actor,
        actor(seat),
        "the responding controller owns this Quick"
    );
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick window");
    };
    assert_eq!(options.len(), 1, "only this spell can respond");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Quick(Some(options[0].handle)),
    )
    .unwrap()
}

fn allocate_three(game: &mut Game, next: CardPlay, handles: &[u64]) {
    assert!(
        handles.len() >= 2,
        "two recipients are needed for this allocation"
    );
    let CardPlay::Awaiting(allocation) = next else {
        panic!("allocation");
    };
    assert!(
        matches!(&allocation.domain,InputDomain::Distribution(d) if d.total==Nat(3)),
        "the printed budget is three"
    );
    let before = format!("{game:?}");
    game.answer_internal_card(
        allocation.actor,
        allocation.token,
        Answer::Distribution(vec![
            DamageShare {
                handle: handles[0],
                amount: Nat(1),
            },
            DamageShare {
                handle: handles[1],
                amount: Nat(1),
            },
        ]),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "rejected input must preserve authority state"
    );
    let result = resolved(
        game.answer_internal_card(
            allocation.actor,
            allocation.token,
            Answer::Distribution(vec![
                DamageShare {
                    handle: handles[0],
                    amount: Nat(1),
                },
                DamageShare {
                    handle: handles[1],
                    amount: Nat(2),
                },
            ]),
        )
        .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|f| matches!(f, Fact::Damage(_)))
            .count(),
        2,
        "each chosen follower receives one damage instruction"
    );
}

#[test]
fn fire_chain_chooses_at_most_two_and_assigns_exactly_three_damage() {
    for (seat, quick, chosen) in [("P1", false, 2_usize), ("P2", true, 2), ("P1", false, 0)] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(if quick { opponent } else { seat });
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":"EBD02-015"}]);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"][opponent]["zones"]["field"] = json!([
            {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"},{"id":"third","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let CardPlay::Awaiting(question) = play(&mut game, seat, quick) else {
            panic!("targets");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("target domain");
        };
        assert_eq!(domain.members.len(), 3);
        let handles = domain
            .members
            .iter()
            .map(|card| card.handle)
            .collect::<Vec<_>>();
        let before_targets = format!("{game:?}");
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(handles.clone()),
        )
        .unwrap_err();
        assert!(
            format!("{game:?}") == before_targets,
            "rejected input must preserve authority state"
        );
        let next = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(handles[..chosen].to_vec()),
            )
            .unwrap();
        if chosen == 0 {
            resolved(next);
        } else {
            allocate_three(&mut game, next, &handles);
        }
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        let mut enemy_damage = [
            state["objects"]["first"]["damage"].as_u64().unwrap(),
            state["objects"]["second"]["damage"].as_u64().unwrap(),
            state["objects"]["third"]["damage"].as_u64().unwrap(),
        ];
        enemy_damage.sort_unstable();
        assert_eq!(
            enemy_damage,
            if chosen == 0 { [0, 0, 0] } else { [0, 1, 2] }
        );
        assert_eq!(state["objects"]["ally"]["hp"], 5_i64);
        assert_eq!(state[opponent]["leader"]["life"], 20_u64);
        assert_eq!(state[seat]["pp"]["current"], 8_u64);
        assert_eq!(state[seat]["cemetery"], json!(["spell"]));
    }
}
