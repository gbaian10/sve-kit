#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the real kind change scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending};
use super::*;
use sve_engine::game::internal::SearchAnswer;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::ObservedZone;

fn build(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "kind-change",
    )
    .unwrap()
}
fn next_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if game.internal_quick_choice().unwrap().is_none() {
        game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
            .unwrap();
    }
    let active = game.projection(View::Referee).unwrap()["turn"]["active"].clone();
    for _ in 0..4_u8 {
        if game.projection(View::Referee).unwrap()["turn"]["active"] != active {
            return;
        }
        let Some(question) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("end turn did not reach the next main phase");
}
fn initial() -> Value {
    let mut position = draw_position();
    position["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw","card":"BP01-171"},{"id":"seraph","card":"BP02-091"}]);
    position["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy-draw","card":"BP01-171"}]);
    position
}
#[expect(
    clippy::unwrap_in_result,
    reason = "Only absent rules stats return None; a missing fixture card must fail the test."
)]
fn field_stats(game: &Game, player: Viewer, number: &str) -> Option<(i64, i64)> {
    let observed = game.observe(player).unwrap();
    let card = observed
        .players
        .iter()
        .find(|seat| seat.player == player)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards
        .iter()
        .map(|entry| &entry.card.properties)
        .find(|properties| properties.card == number)
        .unwrap();
    card.state
        .as_ref()
        .unwrap()
        .stats
        .map(|stats| (stats.attack, stats.life))
}

#[test]
fn lapis_reaches_four_prayers_at_main_and_can_attack_to_find_its_real_seraph() {
    let mut position = initial();
    position["turn"]["active"] = json!("P2");
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":3_u8}}}]);
    let mut game = build(&position);
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), None);
    next_turn(&mut game);
    let prayer = pending(&mut game, "BP16-093");
    resolved(finish(&mut game, prayer));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), Some((5, 5)));
    game.internal_rule_action(&json!({"do":"attack","attacker":"lapis","target":"P2.leader"}))
        .unwrap();
    let search = pending(&mut game, "BP16-093");
    let q = union_burst_batch1::question(search);
    let InputDomain::Search(domain) = &q.domain else {
        panic!("printed Seraph search")
    };
    let found = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap();
    finish(&mut game, found);
    finish_checks(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("seraph"))
    );
}

#[test]
fn ezdia_keeps_a_converted_cards_lastword_and_resets_its_kind_on_reentry() {
    let mut position = initial();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"ezdia","card":"BP05-001"}]);
    position["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..3_u8)
            .map(|i| json!({"id":format!("hunter-{i}"),"card":"BP01-018"}))
            .collect::<Vec<_>>()
    );
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"mordecai","card":"BP01-107"}]);
    let mut game = build(&position);
    let played = game
        .play_internal_card("ezdia", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, played);
    finish_checks(&mut game);
    let fanfare = pending(&mut game, "BP05-001");
    resolved(finish(&mut game, fanfare));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P2, "BP01-107"), None);
    next_turn(&mut game);
    let abilities = game.internal_activated_sources("mordecai").unwrap();
    assert_eq!(abilities.len(), 1);
    let paid = game
        .activate_internal_card_instance("mordecai", abilities[0].instance, PlayerId { seat: 2 })
        .unwrap();
    resolved(finish(&mut game, paid));
    finish_checks(&mut game);
    let revived = pending(&mut game, "BP01-107");
    resolved(finish(&mut game, revived));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P2, "BP01-107"), Some((5, 5)));
}

#[test]
fn lapis_kind_is_live_at_the_printed_four_counter_threshold() {
    for prayers in [2_u8, 3, 4, 5] {
        let mut position = initial();
        position["players"]["P1"]["zones"]["field"] = json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":prayers}}}]);
        let game = build(&position);
        assert_eq!(
            field_stats(&game, Viewer::P1, "BP16-093"),
            (prayers >= 4).then_some((5, 5))
        );
    }
}

#[test]
fn ezdia_with_two_hunters_does_not_change_an_opposing_follower() {
    let mut position = initial();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"ezdia","card":"BP05-001"}]);
    position["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"hunter1","card":"BP01-018"},{"id":"hunter2","card":"BP01-018"}]);
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"mordecai","card":"BP01-107"}]);
    let mut game = build(&position);
    let play = game
        .play_internal_card("ezdia", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    finish_checks(&mut game);
    let fanfare = pending(&mut game, "BP05-001");
    resolved(finish(&mut game, fanfare));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P2, "BP01-107"), Some((5, 5)));
    assert_eq!(
        game.internal_activated_sources("mordecai").unwrap(),
        Vec::new()
    );
}

fn nana_head(game: &mut Game, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Nana evolution trigger")
    };
    let option = options
        .iter()
        .find(|option| option.card == "ECP02-061" && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

#[test]
fn nana_normal_and_super_evolution_keep_both_heads_and_granted_abilities() {
    for (super_evolution, super_first) in [(false, false), (true, false), (true, true)] {
        let mut position = initial();
        position["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        position["players"]["P1"]["leader"]["life"] = json!(10_u8);
        position["players"]["P1"]["sep"] = json!(1_u8);
        position["players"]["P1"]["zones"]["field"] = json!([{"id":"nana","card":"ECP02-060"}]);
        position["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"ECP02-061"}]);
        position["players"]["P1"]["zones"]["hand"] = json!([{"id":"lesson","card":"ECP02-060"}]);
        position["players"]["P2"]["zones"]["field"] =
            json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":4_u8}}}]);
        position["players"]["P2"]["zones"]["hand"] =
            json!([{"id":"discard1","card":"BP01-171"},{"id":"discard2","card":"BP01-171"}]);
        let mut game = build(&position);
        let payment = if super_evolution {
            json!({"pp":1_u8,"sep":1_u8})
        } else {
            json!({"pp":1_u8})
        };
        game.internal_rule_action(
            &json!({"do":"evolve","source":"nana","evolve_card":"form","pay":payment}),
        )
        .unwrap();
        for line in if super_evolution {
            if super_first { vec![2, 1] } else { vec![1, 2] }
        } else {
            vec![1]
        } {
            let play = nana_head(&mut game, line);
            resolved(finish(&mut game, play));
            finish_checks(&mut game);
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P1"]["hand"], json!(["draw"]));
        assert_eq!(
            field_stats(&game, Viewer::P2, "BP16-093"),
            if super_evolution { None } else { Some((5, 5)) }
        );
        if super_evolution {
            next_turn(&mut game);
            // Lapis keeps its printed main-phase trigger as well as the newly granted trigger.
            let printed = pending(&mut game, "BP16-093");
            resolved(finish(&mut game, printed));
            finish_checks(&mut game);
            let granted = pending(&mut game, "ECP02-061");
            resolved(finish(&mut game, granted));
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                18_u8
            );
            assert_eq!(field_stats(&game, Viewer::P2, "BP16-093"), None);
            let sources = game.internal_activated_sources("lapis").unwrap();
            assert_eq!(sources.len(), 1);
            let play = game
                .activate_internal_card_instance("lapis", sources[0].instance, PlayerId { seat: 2 })
                .unwrap();
            resolved(finish(&mut game, play));
            finish_checks(&mut game);
            let after_sacrifice = game.projection(View::Referee).unwrap();
            assert_eq!(after_sacrifice["P2"]["field"], json!([]));
            assert_eq!(after_sacrifice["P2"]["hand"], json!(["enemy-draw"]));
        }
    }
}

fn lapis_selector(index: u64) -> Selector {
    let Action::Move { mut objects, .. } = event_cemetery::move_named(
        ZoneKind::Field,
        OwnerZone::Cemetery,
        "大いなる熾天使・ラピス",
    ) else {
        panic!("named external producer")
    };
    let Selector::Zone {
        binding, predicate, ..
    } = &mut objects
    else {
        panic!("named field selector")
    };
    *binding = Binding::new(index, ScopeId(1));
    let Expr::Compute(predicate) = predicate.as_mut() else {
        panic!("name predicate")
    };
    let Predicate::NamesIntersect { left, .. } = predicate.as_mut() else {
        panic!("name match")
    };
    *left = Expr::Live(Box::new(NamesRead::Names {
        object: ObjectView::Current {
            object: Box::new(ObjectOperand::Bound { binding: *binding }),
        },
    }));
    objects
}
const fn lifetime(end: DurationEnd) -> Duration {
    Duration {
        end,
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    }
}
#[test]
fn temporary_kind_change_preserves_numeric_layers_and_an_existing_granted_keyword() {
    let mut position = initial();
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"lapis","card":"BP16-093","state":{"counters":{"祈りカウンター":4_u8}}}]);
    let program = producer_steps(vec![
        Action::Modify {
            subjects: lapis_selector(51),
            patch: CharacteristicPatch::Combined {
                patches: vec![
                    CharacteristicPatch::Numeric {
                        stat: Stat::Attack,
                        change: NumericChange::Add {
                            amount: Expr::Literal(Int(2)),
                        },
                    },
                    CharacteristicPatch::Numeric {
                        stat: Stat::Life,
                        change: NumericChange::Add {
                            amount: Expr::Literal(Int(3)),
                        },
                    },
                ],
            },
            expires: lifetime(DurationEnd::NoTemporalExpiry {}),
            source: EffectReference::CurrentInstallation {},
        },
        Action::Grant {
            subjects: lapis_selector(52),
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Ward,
            }],
            duration: lifetime(DurationEnd::NoTemporalExpiry {}),
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId("0".into()),
                ability: AbilityId(1),
            },
        },
        Action::Modify {
            subjects: lapis_selector(53),
            patch: CharacteristicPatch::SetKind {
                kind: CardKind::Amulet,
            },
            expires: lifetime(DurationEnd::CurrentTurnEnd {}),
            source: EffectReference::CurrentInstallation {},
        },
    ]);
    let mut game = event_subject_capture::prepared_program(&position, &program);
    let play = game
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play));
    finish_checks(&mut game);
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), None);
    let observed = game.observe(Viewer::P1).unwrap();
    let card = observed.players[0]
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards
        .iter()
        .find(|card| card.card.properties.card == "BP16-093")
        .unwrap();
    assert!(
        card.card
            .properties
            .state
            .as_ref()
            .unwrap()
            .keywords
            .contains(&"守護".to_owned())
    );
    next_turn(&mut game);
    assert_eq!(field_stats(&game, Viewer::P1, "BP16-093"), Some((7, 8)));
}
