use super::*;
use sve_engine::game::internal::{DecisionToken, DecisionView, InstructionOutcome};

#[test]
fn bp01_085_draws_before_private_discard_and_reveals_only_the_discarded_card() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for discard_drawn in [false, true] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"attacker","card":"BP01-085"}]);
            initial["players"][opponent]["zones"]["field"] =
                json!([{"id":"watcher","card":"BP01-085"}]);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"kept","card":"BP01-037"}]);
            initial["players"][seat]["zones"]["deck"] =
                json!([{"id":"drawn","card":"BP01-097"},{"id":"untouched","card":"BP01-037"}]);
            initial["players"][opponent]["zones"]["hand"] =
                json!([{"id":"other","card":"BP01-097"}]);
            let mut game = game(&initial);
            assert_eq!(game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":format!("{opponent}.leader")})).unwrap().outcome, "resolved");
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("own attack");
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].line, 2_u64);
            let CardPlay::Awaiting(question) = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap()
            else {
                panic!("private discard after draw");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("opaque private candidates");
            };
            assert_eq!(domain.members.len(), 2);
            assert_eq!(domain.cardinality.minimum, Nat(1));
            let drawn_state = game.projection(View::Referee).unwrap();
            assert_eq!(drawn_state[seat]["hand"], json!(["kept", "drawn"]));
            assert_eq!(drawn_state[seat]["deck"], json!(["untouched"]));
            let other_view = if seat == "P1" { View::P2 } else { View::P1 };
            assert!(game.internal_decision(other_view).is_none());
            let hidden = game.projection(other_view).unwrap();
            for id in ["kept", "drawn", "untouched"] {
                assert!(hidden["objects"].get(id).is_none());
            }
            reject_answers(&mut game, &question, pending.token, opponent);
            let number = if discard_drawn {
                "BP01-097"
            } else {
                "BP01-037"
            };
            let handle = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(number))
                .unwrap()
                .handle;
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![handle]),
                )
                .unwrap(),
            );
            check_completed(&game, &result, seat, discard_drawn);
        }
    }
}

#[test]
fn bp01_085_keeps_intimidate_against_a_low_cost_attacker() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"small","card":"BP06-048"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"holder","card":"BP01-085","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"small","target":"holder"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

fn reject_answers(
    game: &mut Game,
    question: &DecisionView,
    pending_token: DecisionToken,
    opponent: &str,
) {
    let before = format!("{game:?}");
    for (actor, token, handles) in [
        (
            PlayerId {
                seat: seat_number(opponent),
            },
            question.token,
            vec![0],
        ),
        (question.actor, question.token, vec![99]),
        (question.actor, question.token, vec![]),
        (question.actor, pending_token, vec![0]),
    ] {
        game.answer_internal_card(actor, token, Answer::Targets(handles))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid discard answers preserve the completed draw"
        );
    }
}

fn check_completed(game: &Game, result: &Activation, seat: &str, discard_drawn: bool) {
    let generation = u64::from(discard_drawn);
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Draw(_)))
            .count(),
        1,
        "the attack draws exactly once"
    );
    let discards = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Discard(event) = fact {
                Some(event)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(discards.len(), 1, "exactly one card is discarded");
    assert_eq!(
        discards[0].player.seat,
        seat_number(seat),
        "discard belongs to the attacking controller"
    );
    assert_eq!(
        discards[0].card_before.generation,
        Generation(generation),
        "discard refers to the hand generation before movement"
    );
    assert_eq!(
        discards[0].actual_to.kind,
        ZoneKind::Cemetery,
        "discard actually enters cemetery"
    );
    let printed_stat = if discard_drawn { 7_i64 } else { 5_i64 };
    assert_eq!(
        discards[0].characters.attack,
        Info::Present(Int(printed_stat)),
        "hand events retain printed attack"
    );
    assert_eq!(
        discards[0].characters.life,
        Info::Present(Int(printed_stat)),
        "hand events retain printed life"
    );
    let outcomes = result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Discard(discard) = outcome {
                Some(discard)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes.len(),
        1,
        "one instruction produces one discard outcome"
    );
    assert_eq!(
        outcomes[0].successful.len(),
        1,
        "one card actually leaves hand"
    );
    assert_eq!(
        outcomes[0].moves[0].after.unwrap().generation,
        Generation(generation.checked_add(1).unwrap()),
        "discard creates the next generation"
    );
    check_projection(game, seat, discard_drawn);
}

fn check_projection(game: &Game, seat: &str, discard_drawn: bool) {
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    let other_view = if seat == "P1" { View::P2 } else { View::P1 };
    let (number, chosen, unchosen) = if discard_drawn {
        ("BP01-097", "drawn", "kept")
    } else {
        ("BP01-037", "kept", "drawn")
    };
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state[seat]["hand"],
        json!([unchosen]),
        "only the unselected card stays in hand"
    );
    assert_eq!(
        state[seat]["cemetery"],
        json!([chosen]),
        "only the selected card enters cemetery"
    );
    assert_eq!(
        state[opponent]["hand"],
        json!(["other"]),
        "the other hand is untouched"
    );
    assert_eq!(
        state["semantic_state"]["counters_this_turn"][seat]["discarded"], 1_u8,
        "ordinary discard history increments once"
    );
    let public = game.projection(other_view).unwrap();
    assert_eq!(
        public["objects"][chosen]["card"], number,
        "the discarded identity is public"
    );
    assert!(
        public["objects"].get(unchosen).is_none(),
        "the unselected hand identity stays private"
    );
    assert!(
        public["objects"].get("other").is_some(),
        "the viewer still knows its own hand"
    );
    assert!(
        public["objects"].get("untouched").is_none(),
        "the remaining deck stays private"
    );
}
