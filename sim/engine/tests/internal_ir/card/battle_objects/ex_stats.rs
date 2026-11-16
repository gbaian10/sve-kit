use super::*;

fn make(actor: Viewer, private_owner: Viewer, secret: &str, numbers: (i64, i64)) -> Game {
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    let hidden = if private_owner == Viewer::P1 {
        "P1"
    } else {
        "P2"
    };
    let mut initial = setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(me);
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["hand"] = json!([
            {"id":format!("private-{player}"),"card":"unit-follower"}
        ]);
    }
    initial["players"][hidden]["zones"]["hand"][0]["card"] = json!(secret);
    let state = json!({"power":numbers.0,"hp":numbers.1,"max_hp":numbers.1});
    initial["players"][me]["zones"]["field"] = json!([
        {"id":"source","card":"unit-follower","state":state}
    ]);
    initial["players"][me]["zones"]["ex"] = json!([
        {"id":"ex-first","card":"unit-follower","state":state},
        {"id":"ex-second","card":"unit-follower","state":state},
        {"id":"ex-amulet","card":"unit-amulet"},
        {"id":"ex-spell","card":"unit-spell"}
    ]);
    let current = ObjectView::Current {
        object: Box::new(ObjectOperand::SourceHolder {}),
    };
    let mut entry = ability(
        1,
        [Stat::Attack, Stat::Life]
            .into_iter()
            .zip([1_u64, 2])
            .map(|(stat, id)| {
                source_information::source_hit(
                    id,
                    source_information::source_read(
                        &current,
                        CharacteristicView::Effective,
                        Some(stat),
                    ),
                )
            })
            .collect(),
    );
    entry.node.active_zones = vec![ZoneKind::Ex];
    position(
        Arc::clone(&fixture().catalog),
        &initial,
        Some(&card(vec![entry])),
    )
}

fn read_numbers(game: &mut Game, actor: Viewer) -> (i64, i64) {
    let result = resolved(
        game.activate_internal_card("ex-first", AbilityId(1), actor.player())
            .unwrap(),
    );
    let values = result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(event) = fact {
                assert_eq!(
                    event.source.object, result.source.holder,
                    "the EX holder owns each Current-based damage instruction"
                );
                Some(i64::try_from(event.amount.0.get()).unwrap())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2, "both Current stats produced actual damage");
    game.resume_internal_card().unwrap();
    (values[0], values[1])
}

fn assert_views(game: &Game, actor: Viewer, attack: i64, life: i64) {
    let before = exact_authority(game);
    for viewer in [Viewer::P1, Viewer::P2] {
        let native = game.observe(viewer).unwrap();
        let zones = &native
            .players
            .iter()
            .find(|p| p.player == actor)
            .unwrap()
            .zones;
        let ex = zones
            .iter()
            .find(|z| z.kind == native::ObservedZone::Ex)
            .unwrap();
        assert_eq!(
            ex.cards.len(),
            4,
            "both seats identify every public EX card"
        );
        for (index, member) in ex.cards.iter().enumerate() {
            let expected = (index < 2).then_some(native::Stats { attack, life });
            assert_eq!(
                member.card.properties.state.as_ref().unwrap().stats,
                expected,
                "EX followers expose current numbers and non-followers have none"
            );
        }
        let field = zones
            .iter()
            .find(|z| z.kind == native::ObservedZone::Field)
            .unwrap();
        assert_eq!(
            field.cards[0].card.properties.state.as_ref().unwrap().stats,
            Some(native::Stats { attack, life }),
            "field numbers retain their existing calculation"
        );
        let wire = game
            .battle_observation(viewer, &fixture().references)
            .unwrap();
        let player = wire
            .players
            .iter()
            .find(|p| {
                p.seat
                    == if actor == Viewer::P1 {
                        Seat::P1
                    } else {
                        Seat::P2
                    }
            })
            .unwrap();
        assert_eq!(
            player.zones.ex.cards.len(),
            4,
            "wire retains the full public EX membership"
        );
        for (index, member) in player.zones.ex.cards.iter().enumerate() {
            let value = &wire.objects[&member.object];
            assert_eq!(
                value.knowledge,
                wire::Knowledge::Public,
                "opponent EX cards are public too"
            );
            assert_eq!(
                value
                    .state
                    .as_ref()
                    .unwrap()
                    .stats
                    .map(|s| (s.attack.get(), s.life.get())),
                (index < 2).then_some((attack, life)),
                "wire carries the authoritative numbers"
            );
        }
        let value = &wire.objects[&player.zones.field.cards[0].object];
        assert_eq!(
            value
                .state
                .as_ref()
                .unwrap()
                .stats
                .map(|s| (s.attack.get(), s.life.get())),
            Some((attack, life)),
            "each zone matches its authoritative Current reads"
        );
        for member in &wire.players[usize::from(viewer != Viewer::P1)]
            .zones
            .hand
            .cards
        {
            assert_eq!(
                wire.objects[&member.object].state.as_ref().unwrap().stats,
                None,
                "own hand followers do not gain published numbers"
            );
        }
    }
    assert!(
        exact_authority(game) == before,
        "observing current numbers is read-only"
    );
}

#[test]
fn ex_display_matches_effect_current_reads_for_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        // These seeded states have no damage or off-field numeric installations.
        for numbers in [(2, 3), (7, 8)] {
            let mut game = make(actor, actor, "unit-follower", numbers);
            assert_views(&game, actor, numbers.0, numbers.1);
            let read = read_numbers(&mut game, actor);
            assert_eq!(
                read, numbers,
                "Current reads state rather than printed fallback"
            );
            assert_views(&game, actor, read.0, read.1);
        }
    }
}

#[test]
fn opposing_hand_secrets_do_not_change_the_full_ex_board_for_either_seat() {
    for actor in [Viewer::P1, Viewer::P2] {
        for viewer in [Viewer::P1, Viewer::P2] {
            let secret_owner = if viewer == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            };
            let mut first = make(actor, secret_owner, "unit-spell", (7, 8));
            let mut second = make(actor, secret_owner, "unit-amulet", (7, 8));
            for game in [&mut first, &mut second] {
                assert_eq!(read_numbers(game, actor), (7, 8));
            }
            let before = exact_authority(&first);
            assert_eq!(
                first.observe(viewer).unwrap(),
                second.observe(viewer).unwrap()
            );
            assert_eq!(
                first
                    .battle_observation(viewer, &fixture().references)
                    .unwrap(),
                second
                    .battle_observation(viewer, &fixture().references)
                    .unwrap()
            );
            assert!(
                exact_authority(&first) == before,
                "a hidden hand cannot affect the public numeric projection"
            );
        }
    }
}
