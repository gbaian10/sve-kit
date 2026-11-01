#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed Played scenarios."
)]
use super::event_cemetery::{finish_checks, move_material, position};
use super::*;

fn prepared(initial: &Value, action: Action) -> Game {
    let mut initial = initial.clone();
    // Printed draws need real cards so deck exhaustion cannot clear unrelated pending abilities.
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw-1","card":"movement-unit"},
        {"id":"draw-2","card":"movement-unit"}
    ]);
    event_cemetery::prepared(&initial, action)
}

fn complete(game: &mut Game, mut play: CardPlay, target: &str) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            let member = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(target))
                .unwrap();
            Answer::Targets(vec![member.handle])
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("only the printed target or Ward posture may be requested");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_) | CardPlay::Resolved(_)),
        "a paid card or pending ability must complete"
    );
    finish_checks(game);
}
fn play(game: &mut Game, id: &str, target: &str) {
    let result = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    complete(game, result, target);
}
fn assert_play_waits(game: &mut Game, id: &str) {
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"].clone();
    assert!(
        game.play_internal_card(id, PlayerId { seat: 1 }).is_err(),
        "an unresolved printed occurrence blocks another proper card play"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        before,
        "a rejected interleaving changes no payment"
    );
}
fn settle(game: &mut Game, card: &str, target: &str) -> usize {
    finish_checks(game);
    let mut count = 0_usize;
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("a real printed pending occurrence");
        };
        assert!(
            options.iter().all(|option| option.card == card),
            "only the actual printed observer may subscribe"
        );
        let result = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        complete(game, result, target);
        count = count.checked_add(1).unwrap();
    }
    count
}
fn game(card: &str, material: &str) -> Game {
    prepared(
        &position(card, "P1", material, "P1", "ex"),
        move_material(ZoneKind::Field, OwnerZone::Cemetery),
    )
}

#[test]
fn evolved_gino_buffs_one_allied_follower_after_a_real_amulet_play() {
    for card in GINO {
        let mut game = game(card, "movement-amulet");
        let before = game.projection(View::Referee).unwrap()["objects"]["source"]["hp"]
            .as_u64()
            .unwrap();
        play(&mut game, "material", card);
        assert_eq!(
            settle(&mut game, card, card),
            1,
            "the amulet play creates one observer occurrence"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["hp"],
            before + 1
        );
    }
}
#[test]
fn sylphid_heals_for_another_real_spirit_play_but_not_a_wizard_spell() {
    for card in SYLPHID {
        for (material, expected) in [("BP20-005", true), ("played-spell", false)] {
            let mut initial = position(card, "P1", material, "P1", "ex");
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            let mut game = prepared(
                &initial,
                move_material(ZoneKind::Field, OwnerZone::Cemetery),
            );
            play(&mut game, "material", "movement-enemy");
            assert_eq!(
                settle(&mut game, card, "movement-enemy"),
                usize::from(expected)
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                10_u8 + u8::from(expected)
            );
        }
    }
}
#[test]
fn rinckle_requires_the_played_spell_to_have_the_arcana_trait() {
    for card in RINCKLE {
        for (material, expected) in [("played-spell", true), ("CP02-T02", false)] {
            let mut game = game(card, material);
            play(&mut game, "material", "movement-enemy");
            assert_eq!(
                settle(&mut game, card, "movement-enemy"),
                usize::from(expected)
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if expected { 3_u8 } else { 0_u8 }
            );
        }
    }
}
#[test]
fn sarina_observes_normal_and_token_spells_but_not_follower_plays() {
    for card in SARINA {
        for (material, expected) in [
            ("played-spell", true),
            ("CP02-T02", true),
            ("movement-unit", false),
        ] {
            let mut game = game(card, material);
            play(&mut game, "material", "movement-enemy");
            assert_eq!(
                settle(&mut game, card, "movement-enemy"),
                usize::from(expected)
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if expected { 2_u8 } else { 0_u8 }
            );
        }
    }
}
#[test]
fn machine_lifeform_draws_only_for_the_first_real_repair_spell_of_the_turn() {
    for card in MACHINE {
        let mut initial = position(card, "P1", "BP07-T02", "P1", "ex");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["ex"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second","card":"BP07-T02"}));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        let mut game = prepared(
            &initial,
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
            .as_u64()
            .unwrap();
        let follower_hp =
            game.projection(View::Referee).unwrap()["objects"]["source"]["hp"].clone();
        for (id, expected) in [("material", 1_usize), ("second", 0_usize)] {
            play(&mut game, id, card);
            assert_eq!(
                settle(&mut game, card, "movement-enemy"),
                expected,
                "the shared per-turn quota applies to the actual observer"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            before + 2,
            "both actual RepairMode spells heal the leader"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["hp"],
            follower_hp,
            "RepairMode does not buff the observing follower"
        );
    }
}
#[test]
fn tachyon_reduces_both_stats_of_the_chosen_enemy_after_a_spell_play() {
    for card in TACHYON {
        let mut game = game(card, "played-spell");
        play(&mut game, "material", "movement-enemy");
        assert_eq!(settle(&mut game, card, "movement-enemy"), 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["power"], 9_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 19_u8);
    }
}

#[test]
fn ordinal_cards_count_five_real_plays_and_reject_interleaving_before_pending_resolves() {
    for (cards, fairy) in [(FAIRY, true), (ANASTASIA, false), (ANASTASIA_PR, false)] {
        for card in cards {
            let mut initial = position(card, "P1", "played-spell", "P1", "hand");
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            let mut hand = vec![json!({"id":"source","card":card})];
            hand.extend(
                (1_u8..=4).map(|n| json!({"id":format!("spell-{n}"),"card":"played-spell"})),
            );
            initial["players"]["P1"]["zones"]["hand"] = json!(hand);
            initial["players"]["P1"]["zones"]["ex"] =
                json!([{"id":"next-spell","card":"played-spell"}]);
            let mut game = prepared(
                &initial,
                move_material(ZoneKind::Field, OwnerZone::Cemetery),
            );
            play(&mut game, "source", "movement-enemy");
            assert_eq!(
                settle(&mut game, card, "movement-enemy"),
                usize::from(!fairy),
                "only Anastasia's printed Fanfare observes its own entry"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if fairy { 9_u8 } else { 7_u8 }
            );
            let baseline = game.projection(View::Referee).unwrap()["objects"]["source"].clone();
            for n in 1_u8..=4 {
                play(&mut game, &format!("spell-{n}"), "movement-enemy");
                if n == 2 {
                    assert_play_waits(&mut game, "spell-3");
                }
                if n == 4 {
                    assert_play_waits(&mut game, "next-spell");
                }
                assert_eq!(
                    settle(&mut game, card, "movement-enemy"),
                    1,
                    "the source observes each later proper card play once"
                );
            }
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["pp"]["current"], 5_u8);
            assert_eq!(
                after["P1"]["leader"]["life"],
                if fairy { 11_u8 } else { 10_u8 }
            );
            assert_eq!(
                after["P1"]["hand"].as_array().unwrap().len(),
                usize::from(fairy)
            );
            assert_eq!(
                after["objects"]["source"]["power"],
                baseline["power"].as_u64().unwrap() + u64::from(!fairy)
            );
            assert_eq!(
                after["objects"]["source"]["hp"],
                baseline["hp"].as_u64().unwrap() + u64::from(!fairy)
            );
            play(&mut game, "next-spell", "movement-enemy");
            assert_eq!(
                settle(&mut game, card, "movement-enemy"),
                1,
                "the same sixth card is legal after the fifth occurrence resolves"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                4_u8,
                "the sixth card pays once without repeating the third or fifth reward"
            );
        }
    }
}
const GINO: &[&str] = &["BP05-028"];
const SYLPHID: &[&str] = &["BP20-005", "BP20-P01"];
const RINCKLE: &[&str] = &["BP10-038", "BP10-SL10"];
const SARINA: &[&str] = &["CP02-046", "CP02-P37"];
const MACHINE: &[&str] = &["BP07-044", "PR-171"];
const TACHYON: &[&str] = &["CP01-028", "CP01-SL08", "CP01-SP08"];
const FAIRY: &[&str] = &["BP12-010", "PR-329"];
const ANASTASIA: &[&str] = &["CP02-006", "CP02-P03", "CP02-SP02", "CP02-U02"];
const ANASTASIA_PR: &[&str] = &["PR-435"];
