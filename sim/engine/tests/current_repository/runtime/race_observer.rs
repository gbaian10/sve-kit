#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the complete printed race scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn build(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "race-observer",
    )
    .unwrap()
}
fn position(source: &str, robroy: &str, trainer: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":source}, {"id":"robroy","card":robroy}, {"id":"trainer","card":trainer}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"carrot0","card":"CP01-085"},{"id":"carrot1","card":"CP01-085"},{"id":"carrot2","card":"CP01-085"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!((0_u8..16).map(|n| json!({"id":format!("deck{n}"),"card": if n % 2 == 0 {"BP01-056"} else {"BP01-037"}})).collect::<Vec<_>>());
    initial
}
fn feed(game: &mut Game, number: &str, index: usize, actor: PlayerId) -> Activation {
    let id = selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .filter(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Meal,
                    ..
                }
            )
        })
        .nth(index)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(q) = game.activate_internal_card("source", id, actor).unwrap() else {
        panic!("meal payment")
    };
    let InputDomain::ResourceEntryPayment { materials, .. } = &q.domain else {
        panic!("carrot payment")
    };
    let count = usize::try_from(materials.cardinality.minimum.0).unwrap();
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: materials
                    .members
                    .iter()
                    .take(count)
                    .map(|member| member.handle)
                    .collect(),
                use_ep: false,
            },
        )
        .unwrap(),
    )
}
fn pending(game: &mut Game) -> Option<DecisionView> {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    game.internal_pending_choice().unwrap()
}
fn settle(game: &mut Game, keep: Option<&str>) -> (usize, usize) {
    let mut triggers = 0_usize;
    let mut looks = 0_usize;
    while let Some(menu) = pending(game) {
        let InputDomain::Pending(options) = &menu.domain else {
            panic!("race pending")
        };
        let mut play = game
            .answer_internal_card(menu.actor, menu.token, Answer::Pending(options[0].handle))
            .unwrap();
        triggers = triggers.checked_add(1).unwrap();
        while let CardPlay::Awaiting(q) = play {
            let answer = match &q.domain {
                InputDomain::Targets(domain) => {
                    looks = looks.checked_add(1).unwrap();
                    assert!(
                        game.internal_decision(View::P2).is_none(),
                        "Only the looking player may inspect these cards"
                    );
                    assert!(
                        domain.members.iter().all(|member| matches!(
                            member.card.as_deref(),
                            Some("BP01-056" | "BP01-133")
                        )),
                        "Only spells and amulets may be kept"
                    );
                    Answer::Targets(keep.map_or_else(Vec::new, |number| {
                        vec![
                            domain
                                .members
                                .iter()
                                .find(|member| member.card.as_deref() == Some(number))
                                .unwrap()
                                .handle,
                        ]
                    }))
                }
                InputDomain::Order(cards) => {
                    Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
                }
                other => panic!("race effect question {other:?}"),
            };
            play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        resolved(play);
    }
    (triggers, looks)
}
#[test]
fn each_of_three_real_races_notifies_both_observers_and_keeps_race_time() {
    for (robroy, trainer) in [("CP01-032", "CP01-082"), ("CP01-P23", "CP01-P62")] {
        let mut game = build(&position("CP01-042", robroy, trainer));
        let before = game.projection(View::Referee).unwrap();
        let activation = feed(&mut game, "CP01-042", 2, PlayerId { seat: 1 });
        assert_eq!(
            activation
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Raced(_)))
                .count(),
            3
        );
        let q = pending(&mut game).unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("nine independent occurrences")
        };
        assert_eq!(options.len(), 9);
        assert_eq!(settle(&mut game, Some("BP01-056")), (9, 3));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"].as_i64().unwrap(),
            before["objects"]["source"]["power"].as_i64().unwrap() + 9
        );
        assert_eq!(
            state["objects"]["source"]["max_hp"].as_i64().unwrap(),
            before["objects"]["source"]["max_hp"].as_i64().unwrap() + 6
        );
        assert_eq!(
            state["objects"]["robroy"]["power"],
            before["objects"]["robroy"]["power"]
        );
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 13);
    }
}
#[test]
fn robroy_excludes_its_own_meal_but_trainer_buffs_that_same_subject() {
    let mut initial = position("CP01-032", "CP01-032", "CP01-082");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP01-032"}, {"id":"trainer","card":"CP01-082"}
    ]);
    let mut game = build(&initial);
    feed(&mut game, "CP01-032", 0, PlayerId { seat: 1 });
    assert_eq!(settle(&mut game, None), (1, 0));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 1_u8);
    assert_eq!(state["objects"]["source"]["max_hp"], 3_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}
#[test]
fn robroy_can_keep_an_amulet_or_decline_and_orders_the_unkept_cards() {
    for keep in [None, Some("BP01-133")] {
        let mut initial = position("CP01-038", "CP01-032", "CP01-082");
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"spell","card":"BP01-056"}, {"id":"amulet","card":"BP01-133"},
            {"id":"follower","card":"BP01-037"}, {"id":"last","card":"BP01-037"}, {"id":"tail","card":"BP01-037"}
        ]);
        let mut game = build(&initial);
        feed(&mut game, "CP01-038", 0, PlayerId { seat: 1 });
        assert_eq!(settle(&mut game, keep), (2, 1));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if keep.is_some() {
                json!(["amulet"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P1"]["deck"],
            if keep.is_some() {
                json!(["tail", "last", "follower", "spell"])
            } else {
                json!(["tail", "last", "follower", "amulet", "spell"])
            }
        );
    }
}

#[test]
fn opponent_race_does_not_notify_own_observers() {
    let mut initial = position("CP01-038", "CP01-032", "CP01-082");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"robroy","card":"CP01-032"}, {"id":"trainer","card":"CP01-082"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"source","card":"CP01-038"}]);
    initial["players"]["P2"]["zones"]["evolve_deck"] = json!([{"id":"fuel","card":"CP01-085"}]);
    let mut game = build(&initial);
    feed(&mut game, "CP01-038", 0, PlayerId { seat: 2 });
    assert!(pending(&mut game).is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
        2_u8
    );
}
#[test]
fn trainer_fanfare_requires_a_face_up_exact_carrot_then_flips_and_draws() {
    for (printing, face_up) in [("CP01-082", true), ("CP01-P62", true), ("CP01-082", false)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"carrot","card":"CP01-085","face_up":face_up},
            {"id":"wrong","card":"BP01-172","face_up":true}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
        let mut game = build(&initial);
        let mut entry = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        if let CardPlay::Awaiting(ward) = entry {
            entry = game
                .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                .unwrap();
        }
        assert!(matches!(entry, CardPlay::Entered(_)));
        let menu = pending(&mut game).unwrap();
        let InputDomain::Pending(options) = &menu.domain else {
            panic!("fanfare pending")
        };
        let play = game
            .answer_internal_card(menu.actor, menu.token, Answer::Pending(options[0].handle))
            .unwrap();
        if face_up {
            let CardPlay::Awaiting(q) = play else {
                panic!("formal carrot")
            };
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("carrot target")
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].card.as_deref(), Some("CP01-085"));
            let result = game
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            resolved(result);
        } else {
            assert!(matches!(play, CardPlay::Cancelled));
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["carrot"]["face_up"], false);
        assert_eq!(state["objects"]["wrong"]["face_up"], true);
        assert_eq!(
            state["P1"]["hand"],
            if face_up { json!(["draw"]) } else { json!([]) }
        );
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}

#[test]
fn race_subject_cannot_borrow_another_event_binding() {
    let mut card = selection().current["CP01-082"].program.card().clone();
    let ability = card
        .abilities
        .iter_mut()
        .find(|ability| ability.node.trigger.is_some())
        .unwrap();
    let TriggerCondition::Event {
        pattern: EventPattern::Race { predicate, .. },
    } = &mut ability.node.trigger.as_mut().unwrap().on
    else {
        panic!("race observer")
    };
    let Expr::Compute(operation) = predicate else {
        panic!("subject conjunction")
    };
    let Predicate::And { predicates } = operation.as_mut() else {
        panic!("canonical subject filter")
    };
    predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
        left: Expr::Frozen(Box::new(PlayerFrozen::RaceController {
            event: Binding::new(999, ScopeId(999)),
        })),
        right: Expr::Literal(PlayerRef::Controller {}),
    })));
    WellFormedCardProgram::check(card).unwrap_err();
}
