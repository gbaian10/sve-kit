#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed Play scenarios."
)]
use super::event_cemetery::{finish_checks, move_material, move_named, position, prepared};
use super::*;

fn play_material(game: &mut Game, seat: &str) {
    let play = game
        .play_internal_card(
            "material",
            PlayerId {
                seat: if seat == "P1" { 1 } else { 2 },
            },
        )
        .unwrap();
    assert!(
        matches!(play, CardPlay::Entered(_) | CardPlay::Resolved(_)),
        "the proper card play must finish"
    );
    finish_checks(game);
}

fn resolve(game: &mut Game, card: &str, count: usize, target_card: &str) {
    let pending = game.internal_pending_choice().unwrap();
    if count == 0 {
        assert!(
            pending.is_none(),
            "a nonmatching play produces no occurrence"
        );
        return;
    }
    let pending = pending.unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Play observer")
    };
    assert_eq!(
        options.len(),
        count,
        "each matched play produces one printed occurrence"
    );
    assert!(
        options.iter().all(|option| option.card == card),
        "only the printed observer subscribes"
    );
    let mut play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("printed target")
        };
        let target = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some(target_card))
            .unwrap();
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![target.handle]),
            )
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed trigger has a legal declaration"
    );
    finish_checks(game);
}

fn put_material() -> Action {
    let mut action = move_named(ZoneKind::Ex, OwnerZone::Ex, "movement material");
    let Action::Move { movement, .. } = &mut action else {
        panic!("material placement")
    };
    movement.to = MoveDestination::Shared {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind: ZoneKind::Field,
        },
    };
    action
}

#[test]
fn bp05_027_observes_own_amulet_plays_including_tokens_and_excludes_followers_and_opponents() {
    for card in GINO {
        for (side, material, expected) in [
            ("P1", "movement-amulet", true),
            ("P1", "BP07-T03", true),
            ("P1", "movement-unit", false),
            ("P1", "played-spell", false),
            ("P2", "movement-amulet", false),
        ] {
            let mut game = prepared(
                &position(card, side, material, side, "ex"),
                move_material(ZoneKind::Field, OwnerZone::Cemetery),
            );
            let before = game.projection(View::Referee).unwrap()["objects"]["source"].clone();
            play_material(&mut game, side);
            resolve(&mut game, card, usize::from(expected), "BP05-027");
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(
                after["objects"]["source"]["hp"],
                before["hp"].as_u64().unwrap() + u64::from(expected)
            );
            assert_eq!(after["objects"]["source"]["power"], before["power"]);
        }
    }
}
#[test]
fn bp05_046_observes_the_played_trait_and_does_not_retroactively_trigger_its_new_copy() {
    for card in BELIEVER {
        for (side, material, expected) in [
            ("P1", "BP05-046", true),
            ("P1", "played-spell", true),
            ("P1", "movement-unit", false),
            ("P2", "BP05-046", false),
        ] {
            let mut initial = position(card, side, material, side, "ex");
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            let mut game = prepared(
                &initial,
                move_material(ZoneKind::Field, OwnerZone::Cemetery),
            );
            play_material(&mut game, side);
            resolve(&mut game, card, usize::from(expected), "movement-enemy");
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                10_u8 + u8::from(expected)
            );
        }
    }
}
#[test]
fn cp02_016_counts_ordinary_plays_but_not_effect_placement() {
    for card in OTOHA {
        for (side, material, by_effect, expected) in [
            ("P1", "movement-unit", false, true),
            ("P1", "played-spell", false, true),
            ("P2", "movement-unit", false, false),
            ("P1", "movement-unit", true, false),
        ] {
            let action = if by_effect {
                put_material()
            } else {
                move_material(ZoneKind::Field, OwnerZone::Cemetery)
            };
            let mut game = prepared(&position(card, side, material, side, "ex"), action);
            if by_effect {
                activate(&mut game, "P1");
                finish_checks(&mut game);
            } else {
                play_material(&mut game, side);
            }
            resolve(&mut game, card, usize::from(expected), "movement-enemy");
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                u8::from(expected)
            );
        }
    }
}
const GINO: &[&str] = &["BP05-027"];

const BELIEVER: &[&str] = &["BP05-046", "BP05-P20"];

const OTOHA: &[&str] = &["CP02-016", "CP02-P13", "CSD02b-013"];

#[test]
fn failed_field_or_spell_payment_does_not_produce_a_play_occurrence() {
    for material in ["movement-unit", "played-spell"] {
        let mut initial = position("CP02-016", "P1", material, "P1", "ex");
        initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
        let mut game = prepared(
            &initial,
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        let error = game
            .play_internal_card("material", PlayerId { seat: 1 })
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("insufficient PP for whole-card payment"),
            "the counterexample must fail at the actual payment boundary"
        );
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "failed payment is not a committed play"
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 0_u8);
        assert!(
            after["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("material")),
            "failed payment keeps the actual card in its original EX zone"
        );
    }
}
