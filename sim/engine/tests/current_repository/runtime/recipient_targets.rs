use super::*;

#[test]
fn bp01_168_chooses_opposing_leader_or_follower_and_rejects_invalid_answers() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for leader in [false, true] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-168"}]);
            initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
            initial["players"][opponent]["zones"]["field"] = json!([
                {"id":"enemy","card":"BP01-037"},
                {"id":"amulet","card":"BP19-108"}
            ]);
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
                panic!("target question");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("target domain");
            };
            assert_eq!(
                domain.members.len(),
                2,
                "only the opposing leader and follower are legal"
            );
            assert!(
                domain
                    .members
                    .iter()
                    .all(|member| member.player.seat == seat_number(opponent))
            );
            let chosen = domain
                .members
                .iter()
                .find(|member| member.card.is_none() == leader)
                .unwrap()
                .handle;
            let before = format!("{game:?}");
            for (actor, handles) in [
                (
                    PlayerId {
                        seat: seat_number(opponent),
                    },
                    vec![chosen],
                ),
                (question.actor, vec![]),
                (question.actor, vec![chosen, chosen]),
                (question.actor, vec![99]),
            ] {
                game.answer_internal_card(actor, question.token, Answer::Targets(handles))
                    .unwrap_err();
                assert!(format!("{game:?}") == before, "invalid answers are atomic");
            }
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![chosen]),
                )
                .unwrap(),
            );
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::Damage(_)))
                    .count(),
                1
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[opponent]["leader"]["life"],
                if leader { 17_u8 } else { 20_u8 }
            );
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if leader { 0_u8 } else { 3_u8 }
            );
            assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
            assert_eq!(state[seat]["leader"]["life"], 20_u8);
        }
    }
}

#[test]
fn bp01_168_can_target_the_leader_when_the_opposing_field_is_empty() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-168"}]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(question) = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("leader question");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card, None);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        17_u8
    );
}
