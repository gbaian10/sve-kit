use super::*;
use std::time::Instant;

const MISSING: &[&str] = &["BP06-042", "BP01-173", "BP01-170"];

fn game(initial: &Value) -> Game {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| selection_without_abilities(inputs().clone(), MISSING));
    Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "intentionally-unloaded",
    )
    .unwrap()
}

fn missing_producer_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| selection_without_abilities(producer_inputs(), MISSING))
}

fn views(game: &Game) -> [Value; 3] {
    [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap())
}

#[test]
fn unloaded_public_zone_counts_resolve_while_complete_views_need_ability_sources() {
    for seat in ["P1", "P2"] {
        for zone in ["hand", "deck", "cemetery", "banish", "field"] {
            let mut initial = setup(seat, seat, [20, 20]);
            let number = if zone == "field" {
                "BP01-173"
            } else {
                "BP06-042"
            };
            if zone == "field" {
                initial["players"][seat]["zones"][zone]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"unloaded","card":number}));
            } else {
                initial["players"][seat]["zones"][zone] = json!([{"id":"unloaded","card":number}]);
            }
            let game = game(&initial);
            for view in [View::P1, View::P2, View::Referee] {
                assert_eq!(
                    game.query(view, &format!("{seat}.{zone}_count")).unwrap(),
                    Some(json!(if zone == "field" { 2_u8 } else { 1_u8 }))
                );
            }
            let error = failure(game.projection(View::Referee));
            assert!(error.contains(number));
            assert!(error.contains("current ability definitions unavailable"));
            assert_eq!(game.internal_card_stage(), None);
        }
    }
}

#[test]
fn unloaded_cemetery_movement_requires_replacement_source_evidence() {
    for (seat, owner) in [("P1", "P2"), ("P2", "P1")] {
        let mut initial = setup(seat, seat, [20, 20]);
        initial["players"][owner]["zones"]["cemetery"] =
            json!([{"id":"unloaded","card":"BP06-042","owner":owner}]);
        let action = Action::Move {
            objects: subjects_in(ZoneKind::Cemetery),
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Banish,
                },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        };
        let mut game = with_selected_producer(&initial, action, missing_producer_selection());
        let error = failure(game.activate_internal_card(
            "producer",
            AbilityId(1),
            PlayerId {
                seat: seat_number(seat),
            },
        ));
        assert!(error.contains("unloaded whole card BP06-042/0"));
        assert!(error.contains("movement replacement"));
        assert_eq!(game.internal_card_stage(), None);
        assert_eq!(
            failure(game.activate_internal_card(
                "producer",
                AbilityId(1),
                PlayerId {
                    seat: seat_number(seat)
                }
            )),
            error
        );
    }
}

#[test]
fn unloaded_card_play_activation_and_attack_fail_precisely_without_mutation() {
    for seat in ["P1", "P2"] {
        for number in ["BP06-042", "BP01-173"] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"unloaded","card":number}]);
            let mut game = game(&initial);
            let before = views(&game);
            let error = failure(game.play_internal_card(
                "unloaded",
                PlayerId {
                    seat: seat_number(seat),
                },
            ));
            assert!(error.contains(number));
            assert!(error.contains("holder unloaded generation 0"));
            assert!(error.contains("query card play"));
            assert_eq!(views(&game), before);
            assert_eq!(game.internal_card_stage(), None);
        }
        let mut initial = setup(seat, seat, [20, 20]);
        initial["players"][seat]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"unloaded","card":"BP01-173"}));
        let mut game = game(&initial);
        let before = [View::P1, View::P2, View::Referee].map(|view| failure(game.projection(view)));
        let error = failure(game.activate_internal_card(
            "unloaded",
            AbilityId(777),
            PlayerId {
                seat: seat_number(seat),
            },
        ));
        assert!(error.contains("BP01-173/0"));
        assert!(error.contains("query activate ability 777"));
        assert_eq!(
            [View::P1, View::P2, View::Referee].map(|view| failure(game.projection(view))),
            before
        );
        let target = if seat == "P1" {
            "P2.leader"
        } else {
            "P1.leader"
        };
        let attack_error = failure(game.internal_rule_action(&json!({
            "do":"attack", "attacker":"unloaded", "target":target,
        })));
        assert!(attack_error.contains("BP01-173/0"));
        assert!(attack_error.contains("query attack"));
        assert_eq!(
            [View::P1, View::P2, View::Referee].map(|view| failure(game.projection(view))),
            before
        );
        assert_eq!(game.internal_card_stage(), None);
    }
}

#[test]
fn unloaded_hidden_zone_counts_do_not_reveal_card_identities() {
    for zone in ["hand", "deck"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"][zone] = json!([{"id":"unloaded","card":"BP06-042"}]);
        let game = game(&initial);
        for view in [View::P1, View::P2, View::Referee] {
            assert_eq!(
                game.query(view, &format!("P1.{zone}_count")).unwrap(),
                Some(json!(1_u8))
            );
        }
        let opponent = game.projection(View::P2).unwrap();
        assert!(opponent["objects"].get("unloaded").is_none());
        assert_eq!(opponent["P1"][zone], json!([{"filler":1_u8}]));
    }
}

#[test]
fn a_loaded_eighty_object_position_can_advance_to_the_end_phase() {
    let mut initial = draw_position();
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["field"] =
            json!([{"id":format!("{seat}-field"),"card":NUMBER}]);
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..39_u8)
                .map(|index| json!({"id":format!("{seat}-deck-{index}"),"card":"BP06-048"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = game(&initial);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]
            .as_object()
            .unwrap()
            .len(),
        80
    );
    assert_eq!(game.query(View::P1, "P1.void_count").unwrap(), None);
    let legal_start = Instant::now();
    assert_eq!(game.legal().unwrap(), Vec::<Value>::new());
    let legal = legal_start.elapsed();
    let phase_start = Instant::now();
    let step = game
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let end_phase = phase_start.elapsed();
    assert_eq!(step.outcome, "resolved");
    eprintln!(
        "B1 80 loaded objects: legal={}us end-phase={}us",
        legal.as_micros(),
        end_phase.as_micros()
    );
}

#[test]
fn nonfield_ability_source_discovery_does_not_reuse_prototype_headers() {
    // BP01-170 has a hand discard trigger; its old active_zones cannot prove current irrelevance.
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"unloaded","card":"BP01-170"}]);
    let mut game = game(&initial);
    let before = views(&game);
    let error = failure(game.internal_rule_action(&json!({"do":"end-phase"})));
    assert!(error.contains("BP01-170/0"));
    assert!(error.contains("current ability definitions unavailable"));
    assert_eq!(views(&game), before);
    assert_eq!(game.internal_card_stage(), None);
}

#[test]
fn current_loaded_cards_still_reject_injected_prototype_keywords() {
    let mut initial = setup("P1", "P1", [20, 20]);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"keywords":["guard"]});
    let mut game = game(&initial);
    let before = views(&game);
    let error = failure(game.internal_rule_action(&json!({"do":"end-phase"})));
    assert!(error.contains("prototype keywords"));
    assert_eq!(views(&game), before);
}

#[test]
fn loaded_spell_payment_stops_before_charging_for_unknown_off_field_abilities() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"spell","card":"CP02-068"},
        {"id":"unloaded","card":"BP01-170"},
    ]);
    let mut game = game(&initial);
    let before = views(&game);
    let error = failure(game.play_internal_card("spell", PlayerId { seat: 1 }));
    assert!(error.contains("BP01-170/0"));
    assert!(error.contains("current ability definitions unavailable"));
    assert_eq!(views(&game), before);
    assert_eq!(game.internal_card_stage(), None);
}

#[test]
fn empty_current_selection_does_not_leak_hidden_card_diagnostics_to_players() {
    let (snapshot, keywords, mut input) = inputs().clone();
    input["authors"] = json!([]);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"secret","card":"BP01-170"}]);
    let game = Game::from_selected_effects(
        selected,
        &initial,
        &Value::Null,
        &Value::Null,
        "empty-current-selection",
    )
    .unwrap();
    let authority = failure(game.projection(View::Referee));
    assert!(authority.contains("BP01-170"));
    assert!(authority.contains("holder secret"));
    let player = failure(game.projection(View::P1));
    assert_eq!(
        player,
        "unsupported: current projection requires unavailable abilities"
    );
    assert!(!player.contains("BP01-170"));
    assert!(!player.contains("secret"));
    assert!(!player.contains("generation"));
}
