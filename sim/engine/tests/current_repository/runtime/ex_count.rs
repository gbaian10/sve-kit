#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::DamageShare;

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(domain.members.iter().take(1).map(|m| m.handle).collect())
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("unexpected printed choice: {:?}", question.domain);
            };
            Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed play has legal targets and payment"
    );
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger")
    };
    assert!(!options.is_empty(), "a printed trigger is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
}

fn position(card: &str, material: &str, count: usize, payment: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    if payment {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"payment","card":material}));
    }
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("grave-{i}"),"card":material}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"},{"id":"last","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial
}

fn ex_position(card: &str, count: usize) -> Value {
    let mut initial = position(card, "BP01-037", 0, false);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("ex-{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"opponent-ex","card":"BP01-037"}]);
    initial
}

#[test]
fn ex_count_fanfare_reads_own_ex_size() {
    for card in ["BP02-014"] {
        for count in [0_usize, 3] {
            let mut game = game(&ex_position(card, count));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                1_u64 + u64::try_from(count).unwrap()
            );
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count);
        }
    }
}

#[test]
fn ex_count_named_presence_rejects_wrong_name_owner_and_zone() {
    for card in ["BP03-051", "PR-055"] {
        for pawn in ["BP03-T03", "BP02-T04"] {
            for present in [false, true] {
                let mut initial = ex_position(card, 1);
                if present {
                    initial["players"]["P1"]["zones"]["ex"][0]["card"] = json!(pawn);
                }
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"field-pawn","card":pawn}]);
                initial["players"]["P2"]["zones"]["ex"][0]["card"] = json!(pawn);
                let mut game = game(&initial);
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play);
                trigger(&mut game);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["objects"]["source"]["hp"], 2_u8 + u8::from(present));
                assert_eq!(
                    state["objects"]["source"]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("守護")),
                    present
                );
            }
        }
    }
}

#[test]
fn ex_count_distribution_captures_the_printed_ex_budget() {
    for card in ["BP03-007", "BP03-P01"] {
        for count in [0_usize, 3] {
            let mut initial = ex_position(card, count);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"enemy-two","card":"BP04-117"}));
            let mut game = game(&initial);
            let mut play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            while let CardPlay::Awaiting(question) = play {
                let answer = if let InputDomain::Targets(domain) = &question.domain {
                    Answer::Targets(
                        domain
                            .members
                            .iter()
                            .take(if count == 0 { 0 } else { 2 })
                            .map(|m| m.handle)
                            .collect(),
                    )
                } else if let InputDomain::Distribution(domain) = &question.domain {
                    assert_eq!(domain.total, Nat(3));
                    assert_eq!(domain.members.len(), 2_usize);
                    Answer::Distribution(
                        domain
                            .members
                            .iter()
                            .enumerate()
                            .map(|(i, member)| DamageShare {
                                handle: member.handle,
                                amount: Nat(u64::try_from(i + 1).unwrap()),
                            })
                            .collect(),
                    )
                } else {
                    panic!("printed allocation");
                };
                play = game
                    .answer_internal_card(question.actor, question.token, answer)
                    .unwrap();
            }
            assert!(!matches!(play, CardPlay::Cancelled));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], u8::from(count != 0));
            assert_eq!(
                state["objects"]["enemy-two"]["damage"],
                if count == 0 { 0_u8 } else { 2_u8 }
            );
        }
    }
}
