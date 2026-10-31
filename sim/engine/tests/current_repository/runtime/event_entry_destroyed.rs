#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed event scenarios."
)]
use super::event_cemetery::{finish_checks, move_material, position, prepared, resolve_event};
use super::*;

fn play_material(game: &mut Game, side: &str, id: &str) {
    let mut play = game
        .play_internal_card(
            id,
            PlayerId {
                seat: if side == "P1" { 1 } else { 2 },
            },
        )
        .unwrap();
    if let CardPlay::Awaiting(question) = &play {
        assert!(
            matches!(question.domain, InputDomain::WardEntry { .. }),
            "only the printed entry posture precedes field entry"
        );
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the public play enters the material"
    );
    finish_checks(game);
}

fn material_subjects() -> Selector {
    let Action::Move { objects, .. } = move_material(ZoneKind::Field, OwnerZone::Cemetery) else {
        panic!("material movement")
    };
    objects
}

fn create_token(name: &str) -> Action {
    Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplateId(name.into()),
            name: Expr::Literal(CardName(name.into())),
            count: Expr::Literal(Nat(1)),
        }],
        destination: TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Field,
            }),
        },
        output: Binding::new(50, ScopeId(1)),
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects: vec![],
    }
}

#[test]
fn wolves_and_all_printings_observe_only_other_allied_follower_entries() {
    for card in ["BP01-021", "ETD03-017", "SD01-008"] {
        for (side, material, expected) in [
            ("P1", "movement-unit", true),
            ("P2", "movement-unit", false),
            ("P1", "movement-amulet", false),
        ] {
            let mut game = prepared(
                &position(card, side, material, side, "ex"),
                move_material(ZoneKind::Field, OwnerZone::Cemetery),
            );
            let before = game.projection(View::Referee).unwrap()["objects"]["source"].clone();
            play_material(&mut game, side, "material");
            resolve_event(&mut game, card, usize::from(expected));
            let after = game.projection(View::Referee).unwrap();
            for stat in ["power", "hp"] {
                assert_eq!(
                    after["objects"]["source"][stat],
                    before[stat].as_u64().unwrap() + u64::from(expected),
                    "only another allied follower grants both stats"
                );
            }
        }
        let mut initial = position(card, "P1", "movement-unit", "P1", "ex");
        initial["players"]["P1"]["zones"]["ex"] = initial["players"]["P1"]["zones"]["field"].take();
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        let mut game = prepared(
            &initial,
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        play_material(&mut game, "P1", "source");
        resolve_event(&mut game, card, 0);
    }
}

#[test]
fn prayer_candelabra_filters_amulets_and_admits_only_once_per_turn() {
    for (side, material, expected) in [
        ("P1", "movement-amulet", true),
        ("P1", "movement-unit", false),
        ("P2", "movement-amulet", false),
    ] {
        let mut initial = position("BP04-115", side, material, side, "ex");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"][side]["zones"]["ex"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second","card":material}));
        let mut game = prepared(
            &initial,
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        play_material(&mut game, side, "material");
        resolve_event(&mut game, "BP04-115", usize::from(expected));
        play_material(&mut game, side, "second");
        resolve_event(&mut game, "BP04-115", 0);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            10_u8 + u8::from(expected),
            "only the first matching amulet entry heals"
        );
    }
}

#[test]
fn blade_hedgehog_distinguishes_destruction_from_burial_and_observes_rule_destruction() {
    for (active, side, method, expected) in [
        ("P1", "P2", "destroy", true),
        ("P1", "P2", "damage", true),
        ("P1", "P2", "put", false),
        ("P1", "P2", "return", false),
        ("P1", "P1", "destroy", false),
        ("P2", "P2", "destroy", false),
    ] {
        let action = match method {
            "destroy" => Action::Destroy {
                objects: material_subjects(),
            },
            "damage" => Action::Damage {
                recipients: RecipientSet::Objects {
                    objects: material_subjects(),
                },
                amount: DamageAmount::Uniform {
                    amount: Expr::Literal(Int(20)),
                },
                flags: DamageFlags::Ability,
                source: ObjectView::SourceInformation {},
                batch: BatchPolicy::Instruction {},
            },
            "put" => move_material(ZoneKind::Field, OwnerZone::Cemetery),
            "return" => move_material(ZoneKind::Field, OwnerZone::Hand),
            _ => panic!("known movement method"),
        };
        let mut game = prepared(
            &position("BP03-034", active, "movement-unit", side, "field"),
            action,
        );
        let before = game.projection(View::Referee).unwrap()["objects"]["source"].clone();
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP03-034", usize::from(expected));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["objects"]["source"]["power"],
            before["power"].as_u64().unwrap() + u64::from(expected),
            "only opposing destruction on the controller's turn grants power"
        );
        assert_eq!(
            after["objects"]["source"]["hp"], before["hp"],
            "the destruction observer never increases health"
        );
    }
    let mut game = prepared(
        &position("BP03-034", "P1", "movement-amulet", "P2", "field"),
        Action::Destroy {
            objects: material_subjects(),
        },
    );
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP03-034", 0);
}

#[test]
fn dragon_token_entries_filter_class_and_do_not_count_another_non_token_dragon() {
    for (name, expected) in [("ドラゴン", true), ("操り人形", false)] {
        let mut initial = position("BP14-067", "P1", "movement-unit", "P1", "ex");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = prepared(&initial, create_token(name));
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, "BP14-067", usize::from(expected));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if expected { 12_u8 } else { 10_u8 },
            "a token of the wrong class does not trigger recovery"
        );
    }
    let mut initial = position("BP14-067", "P1", "BP14-067", "P1", "ex");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = prepared(
        &initial,
        move_material(ZoneKind::Field, OwnerZone::Cemetery),
    );
    play_material(&mut game, "P1", "material");
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("non-token dragon fanfare")
    };
    assert_eq!(
        options.len(),
        1,
        "only the entered card's fanfare is pending"
    );
    assert_eq!(
        options[0].line, 3,
        "the non-token dragon does not produce either recovery observer"
    );
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("optional fanfare payment")
    };
    assert_eq!(payment.domain, InputDomain::OptionalCost);
    assert_eq!(
        game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(false))
            .unwrap(),
        CardPlay::Cancelled
    );
    finish_checks(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no recovery occurrence was admitted for the non-token dragon"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_u8
    );
}

#[test]
fn dragon_fanfare_creates_a_token_and_both_printed_observers_recover_life() {
    let mut initial = position("BP14-067", "P1", "BP14-067", "P1", "ex");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = prepared(
        &initial,
        move_material(ZoneKind::Field, OwnerZone::Cemetery),
    );
    play_material(&mut game, "P1", "material");
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed dragon fanfare")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].line, 3);
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("printed dragon payment")
    };
    assert_eq!(payment.domain, InputDomain::OptionalCost);
    let play = game
        .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
        .unwrap();
    assert!(!matches!(play, CardPlay::Cancelled));
    finish_checks(&mut game);
    resolve_event(&mut game, "BP14-067", 2);
    resolve_event(&mut game, "BP14-067", 1);
    resolve_event(&mut game, "BP14-067", 0);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["leader"]["life"], 14_u8);
    assert_eq!(after["P1"]["pp"]["current"], 4_u8);
    assert!(
        after["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| { after["objects"][id.as_str().unwrap()]["card"] == "BP01-T11" })
    );
}

#[test]
fn punish_sister_observes_every_allied_amulet_departure_and_keeps_a_departed_source() {
    for (side, material, to, simultaneous, expected) in [
        ("P1", "movement-amulet", OwnerZone::Cemetery, false, true),
        ("P1", "movement-amulet", OwnerZone::Hand, false, true),
        ("P1", "movement-amulet", OwnerZone::Banish, false, true),
        ("P1", "movement-amulet", OwnerZone::Ex, false, true),
        ("P2", "movement-amulet", OwnerZone::Cemetery, false, false),
        ("P1", "movement-unit", OwnerZone::Cemetery, false, false),
        ("P1", "movement-amulet", OwnerZone::Cemetery, true, true),
    ] {
        let mut action = move_material(ZoneKind::Field, to);
        if simultaneous {
            let Action::Move {
                objects: Selector::Zone { predicate, .. },
                ..
            } = &mut action
            else {
                panic!("public movement")
            };
            let Expr::Compute(predicate) = predicate.as_mut() else {
                panic!("material name predicate")
            };
            let Predicate::NamesIntersect {
                right: Expr::Literal(names),
                ..
            } = predicate.as_mut()
            else {
                panic!("material names")
            };
            names.push(CardName("パニッシュシスター".into()));
        }
        let repeat = expected && !simultaneous && to == OwnerZone::Cemetery;
        let mut initial = position("BP04-111", "P1", material, side, "field");
        if repeat {
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"second","card":material}]);
        }
        let mut game = prepared(&initial, action);
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, "BP04-111", usize::from(expected));
        if repeat {
            play_material(&mut game, "P1", "second");
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve_event(&mut game, "BP04-111", 0);
        }
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["objects"]["enemy"]["damage"],
            if expected { 2_u8 } else { 0_u8 },
            "the matched departure deals its printed targeted damage"
        );
        if simultaneous {
            assert!(
                after["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                "the printed observer resolves using its sealed information after leaving"
            );
        }
    }
}
