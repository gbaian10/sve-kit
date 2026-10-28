#![expect(
    clippy::panic,
    reason = "A missing pending question fails the scenario immediately."
)]

use super::*;

fn opposite(seat: &str) -> &str {
    if seat == "P1" { "P2" } else { "P1" }
}

fn resolve_trigger(game: &mut Game, number: &str) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending damage ability");
    };
    assert_eq!(
        options.len(),
        1,
        "only the current card's own trigger is pending"
    );
    assert_eq!(
        options[0].card, number,
        "the current source owns the pending ability"
    );
    let handle = options[0].handle;
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId {
            seat: if question.actor.seat == 1 { 2 } else { 1 },
        },
        question.token,
        Answer::Pending(handle),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "the other actor cannot resolve this ability"
    );
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn leader_hits(result: &Activation, seat: &str, amount: u64) {
    let hits = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        matches!(hits.as_slice(), [hit] if hit.recipient == (RuntimeDamageRecipient::Leader { player: PlayerId { seat: seat_number(seat) } }) && hit.amount.0.get() == amount),
        "exactly the opposing leader receives the printed ability damage"
    );
}

#[test]
fn bp15_064_hits_its_own_target_then_only_opposing_leaders_and_draws() {
    for (seat, hp) in [("P1", 1_u8), ("P2", 1), ("P1", 5), ("P2", 5)] {
        let opponent = opposite(seat);
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":"BP15-064"}]);
        initial["players"][seat]["zones"]["deck"] = json!([{"id":"drawn","card":"CP02-068"}]);
        let mut no_target = game(&initial);
        let before = format!("{no_target:?}");
        no_target
            .play_internal_card(
                "spell",
                PlayerId {
                    seat: seat_number(seat),
                },
            )
            .unwrap_err();
        assert!(
            format!("{no_target:?}") == before,
            "no legal ally rejects before payment"
        );
        initial["players"][seat]["zones"]["field"] =
            json!([{"id":"ally","card":"BP01-037","state":{"hp":hp}}]);
        let mut game = game(&initial);
        let CardPlay::Awaiting(question) = game
            .play_internal_card(
                "spell",
                PlayerId {
                    seat: seat_number(seat),
                },
            )
            .unwrap()
        else {
            panic!("one ally target");
        };
        let result = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Damage(_)))
                .count(),
            2,
            "the two damage instructions each happen once"
        );
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state[seat]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("ally")),
            hp == 1
        );
        assert_eq!(
            state[seat]["leader"]["life"], 20_u8,
            "the controller's leader is not an opponent"
        );
        assert_eq!(
            state[opponent]["leader"]["life"], 19_u8,
            "only the opposing leader loses one life"
        );
        assert_eq!(
            state[seat]["hand"],
            json!(["drawn"]),
            "the final draw completes the spell"
        );
    }
}

#[test]
fn bp19_081_own_end_debuffs_opposing_followers_and_damages_their_leader() {
    for seat in ["P1", "P2"] {
        let opponent = opposite(seat);
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"watcher","card":"BP19-081"}]);
        initial["players"][opponent]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        assert!(game.internal_pending_choice().unwrap().is_none());
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let result = resolve_trigger(&mut game, "BP19-081");
        leader_hits(&result, opponent, 3);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state[seat]["leader"]["life"], 20_u8,
            "the controller's leader remains unchanged"
        );
        assert_eq!(
            state[opponent]["leader"]["life"], 17_u8,
            "the opposing leader receives three ability damage"
        );
        assert_eq!(
            state["objects"]["enemy"]["power"], 2_i64,
            "the opposing follower also receives the printed debuff"
        );
        assert_eq!(
            state["objects"]["enemy"]["hp"], 2_i64,
            "the debuff reduces life independently of leader damage"
        );
    }
}

#[test]
fn bp21_p37_attack_trigger_damages_only_the_opposing_leader() {
    for seat in ["P1", "P2"] {
        let opponent = opposite(seat);
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"attacker","card":"BP21-P37"}]);
        initial["players"][opponent]["zones"]["field"] = json!([{"id":"other","card":"BP21-P37"}]);
        let mut game = game(&initial);
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":format!("{opponent}.leader")}),
        )
        .unwrap();
        let before_effect = game.projection(View::Referee).unwrap()[opponent]["leader"]["life"]
            .as_i64()
            .unwrap();
        let result = resolve_trigger(&mut game, "BP21-P37");
        leader_hits(&result, opponent, 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state[seat]["leader"]["life"], 20_u8,
            "the attack-time ability does not damage its controller"
        );
        assert_eq!(
            state[opponent]["leader"]["life"],
            before_effect.checked_sub(2).unwrap(),
            "the ability adds exactly two damage to any ordinary attack damage"
        );
    }
}
