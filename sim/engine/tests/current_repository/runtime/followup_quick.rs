#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed evolution and Quick sequence."
)]
use super::followup_materials::{board, declare};
use super::*;
use sve_engine::game::internal::{DamageShare, DecisionView};

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the actual linked evolution pays its printed fee"
    );
    checks(game);
    let mut play = declare(game, 2);
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    3,
                    "the three opposing followers are visible before allocating damage"
                );
                assert_eq!(
                    domain.members[0].card.as_deref(),
                    Some("followup-weak"),
                    "the first allocation recipient has two health"
                );
                assert_eq!(
                    domain.members[1].card.as_deref(),
                    Some("followup-neutral"),
                    "the second allocation recipient survives three damage"
                );
                Answer::Targets(vec![domain.members[0].handle, domain.members[1].handle])
            }
            InputDomain::Distribution(domain) => {
                assert_eq!(
                    domain.total,
                    Nat(5),
                    "the printed evolved ability distributes exactly five damage"
                );
                Answer::Distribution(
                    domain
                        .members
                        .iter()
                        .map(|member| DamageShare {
                            handle: member.handle,
                            amount: Nat(if member.card.as_deref() == Some("followup-weak") {
                                2
                            } else {
                                3
                            }),
                        })
                        .collect(),
                )
            }
            _ => panic!("printed evolution allocation"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    resolved(play);
    checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("the evolved departure observer")
    };
    assert_eq!(options.len(), 1, "the old base observer must not also fire");
    assert_eq!(
        options[0].line, 1,
        "the evolved departure observer supplies this occurrence"
    );
    let departure = declare(game, 1);
    resolved(departure);
    checks(game);
}

fn opponent_window(game: &mut Game) -> DecisionView {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let opponent = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(
        opponent.actor,
        PlayerId { seat: 2 },
        "the non-turn player first receives the empty window"
    );
    drop(
        game.answer_internal_card(opponent.actor, opponent.token, Answer::Quick(None))
            .unwrap(),
    );
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    checks(game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(
        q.actor,
        PlayerId { seat: 1 },
        "Medusa now belongs to the non-turn player"
    );
    q
}

fn quick_spell(game: &mut Game) {
    let window = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &window.domain else {
        panic!("Quick window")
    };
    let option = options
        .iter()
        .find(|option| option.line.is_none() && option.card == "BP11-068")
        .unwrap();
    let mut play = game
        .answer_internal_card(
            window.actor,
            window.token,
            Answer::Quick(Some(option.handle)),
        )
        .unwrap();
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(false),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members[0].card.as_deref(),
                    Some("followup-neutral"),
                    "the real Quick spell selects the surviving opposing follower"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            _ => panic!("ordinary printed Quick spell target and optional fee"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    resolved(play);
    checks(game);
}

#[test]
fn sweet_medusa_both_printings_evolve_allocate_create_once_and_discount_only_the_next_quick_card() {
    for printing in ["BP08-036", "BP08-SL08"] {
        for existing in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] =
                json!([{"id":"free","card":"BP11-068"},{"id":"second","card":"BP11-068"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":printing}]);
            let mut field = vec![json!({"id":"source","card":"BP08-035","state":{"acted":false}})];
            if existing {
                field.push(json!({"id":"old-viper","card":"BP04-T01"}));
            }
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"weak","card":"followup-weak"},{"id":"boss","card":"followup-neutral"},{"id":"foreign-viper","card":"BP04-T01"}]);
            initial["players"]["P2"]["zones"]["deck"] =
                json!([{"id":"opponent-draw","card":"followup-neutral"}]);
            let mut game = board(&initial);
            evolve(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"], 4_u8,
                "the actual evolved printing supplies the face"
            );
            assert_eq!(
                state["objects"]["boss"]["hp"], 17_u8,
                "five damage was split two and three"
            );
            assert_eq!(state["P2"]["cemetery"], json!(["weak"]));
            assert_eq!(
                state["P1"]["field"].as_array().unwrap().len(),
                2 + usize::from(existing),
                "one departure creates one real Viper"
            );
            let q = opponent_window(&mut game);
            let InputDomain::Quick { options, .. } = &q.domain else {
                panic!("Quick window")
            };
            let material_option = options.iter().find(|option| option.line == Some(3));
            assert_eq!(
                material_option.is_some(),
                existing,
                "a foreign Viper cannot complete the own cost"
            );
            if let Some(option) = material_option {
                let CardPlay::Awaiting(payment) = game
                    .answer_internal_card(q.actor, q.token, Answer::Quick(Some(option.handle)))
                    .unwrap()
                else {
                    panic!("Viper fee")
                };
                let InputDomain::CostMaterials(domain) = &payment.domain else {
                    panic!("own Vipers")
                };
                assert_eq!(domain.members.len(), 2);
                resolved(
                    game.answer_internal_card(
                        payment.actor,
                        payment.token,
                        Answer::CostMaterials(vec![0, 1]),
                    )
                    .unwrap(),
                );
                checks(&mut game);
                let paid = game.projection(View::Referee).unwrap();
                assert_eq!(paid["P1"]["field"], json!(["source"]));
                assert_eq!(
                    paid["objects"]["source"]["acted"], false,
                    "this cost does not Act its source"
                );
                assert_eq!(
                    paid["P1"]["pp"]["current"], 8_u8,
                    "the material Quick fee has no PP"
                );
                quick_spell(&mut game);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    8_u8,
                    "next printed PP2 card is reduced to zero"
                );
                quick_spell(&mut game);
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(
                    after["P1"]["pp"]["current"], 6_u8,
                    "only the next card receives the reduction"
                );
                assert_eq!(
                    after["objects"]["boss"]["hp"], 11_u8,
                    "evolution and both printed Quick spells dealt all eleven damage"
                );
                assert_eq!(after["P2"]["field"], json!(["boss", "foreign-viper"]));
            }
        }
    }
}

fn evolve_without_departures(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the actual linked evolution pays its printed fee"
    );
    checks(game);
    let mut play = declare(game, 2);
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the surviving boss can receive the evolution damage"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::Distribution(domain) => {
                assert_eq!(
                    domain.total,
                    Nat(5),
                    "the printed evolved ability distributes exactly five damage"
                );
                Answer::Distribution(vec![DamageShare {
                    handle: domain.members[0].handle,
                    amount: Nat(5),
                }])
            }
            _ => panic!("one surviving evolution recipient"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    resolved(play);
    checks(game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "five damage does not kill the twenty-health target"
    );
}

fn act_twice(game: &mut Game, printing: &str) {
    let ability = followup_materials::prepared().current[printing]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 3)
        .unwrap()
        .node
        .id;
    for count in [4_usize, 2] {
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("two Viper fee")
        };
        let InputDomain::CostMaterials(domain) = &q.domain else {
            panic!("Viper cost materials")
        };
        assert_eq!(
            domain.members.len(),
            count,
            "only the remaining own Vipers can pay the next activation"
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0, 1]))
                .unwrap(),
        );
        checks(game);
    }
}

#[test]
fn sweet_medusa_q1073_stacks_two_activations_on_one_next_card_without_discounting_the_second() {
    for printing in ["BP08-036", "BP08-SL08"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"first-card","card":"followup-large"},{"id":"second-card","card":"followup-large"}]);
        let mut field = vec![json!({"id":"source","card":"BP08-035","state":{"acted":false}})];
        field.extend((0..4_u8).map(|n| json!({"id":format!("viper{n}"),"card":"BP04-T01"})));
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"boss","card":"followup-neutral"}]);
        let mut game = board(&initial);
        evolve_without_departures(&mut game);
        act_twice(&mut game, printing);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8,
            "neither activation spends PP or consumes the other installation"
        );
        assert!(matches!(
            game.play_internal_card("first-card", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8,
            "printed six is reduced by both independent installations to two"
        );
        assert!(matches!(
            game.play_internal_card("second-card", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            0_u8,
            "both reductions were consumed by the first card"
        );
    }
}
