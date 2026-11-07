#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed hand-transfer batch."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed decision")
    };
    question
}

fn payment(game: &mut Game, accept: bool) -> CardPlay {
    let mut entry = game.play_internal_card("source", PLAYER).unwrap();
    if let CardPlay::Awaiting(question) = entry {
        assert!(
            matches!(question.domain, InputDomain::WardEntry { .. }),
            "only printed Ward can ask before field entry"
        );
        entry = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "the card enters first"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    assert_eq!(options.len(), 1, "only the source has a printed fanfare");
    let head = awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert!(
        matches!(head.domain, InputDomain::OptionalCost),
        "the controller accepts the head cost first"
    );
    let accepted = game
        .answer_internal_card(head.actor, head.token, Answer::OptionalCost(accept))
        .unwrap();
    if !accept {
        return accepted;
    }
    let material = awaiting(accepted);
    let InputDomain::CostMaterials(domain) = &material.domain else {
        panic!("hand material")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the eligible hand copy can pay"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot inspect the hand cost"
    );
    game.answer_internal_card(
        material.actor,
        material.token,
        Answer::CostMaterials(vec![domain.members[0].handle]),
    )
    .unwrap()
}

#[test]
fn bp20_071_all_prints_restore_three_pp_only_after_the_ocean_hand_cost() {
    for number in ["BP20-071", "BP20-P39"] {
        for accept in [true, false] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"source","card":number},{"id":"material","card":"BP20-071"}
            ]);
            let mut game = game(&initial);
            let step = payment(&mut game, accept);
            if accept {
                resolved(step);
            } else {
                assert!(matches!(step, CardPlay::Cancelled));
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if accept { 8_u8 } else { 5_u8 }
            );
            assert_eq!(
                state["P1"]["ex"],
                if accept {
                    json!(["material"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P1"]["hand"],
                if accept {
                    json!([])
                } else {
                    json!(["material"])
                }
            );
            let opponent = game.projection(View::P2).unwrap();
            if accept {
                assert_eq!(opponent["objects"]["material"]["card"], "BP20-071");
            } else {
                assert!(opponent["objects"]["material"].is_null());
            }
        }
    }
}

fn order(game: &mut Game, step: CardPlay) -> CardPlay {
    let question = awaiting(step);
    let InputDomain::Order(cards) = &question.domain else {
        panic!("private top or bottom ordering")
    };
    assert_eq!(cards.len(), 1, "this partition has one card on each side");
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the controller sees the observed ordering"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Order(vec![cards[0].handle]),
    )
    .unwrap()
}

#[test]
fn cp04_116_all_prints_pay_then_keep_one_of_two_on_top_and_bottom_the_other() {
    for number in ["CP04-116", "CP04-P88", "PR-504"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"material","card":"CP04-116"}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"first","card":"BP01-171"},
            {"id":"second","card":"BP01-034"},
            {"id":"tail","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let question = awaiting(payment(&mut game, true));
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("the private top two")
        };
        assert_eq!(domain.members.len(), 2);
        assert_eq!(domain.cardinality.minimum, Nat(0));
        assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
        assert!(game.internal_decision(View::P2).is_none());
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["material"])
        );
        let picked = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-034"))
            .unwrap()
            .handle;
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![picked]),
            )
            .unwrap();
        let step = order(&mut game, step);
        resolved(order(&mut game, step));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["deck"], json!(["second", "tail", "first"]));
        let opponent = game.projection(View::P2).unwrap();
        for id in ["first", "second", "tail"] {
            assert!(opponent["objects"][id].is_null());
        }
    }
}
