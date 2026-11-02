#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the entry-origin scenarios."
)]
use super::*;

fn initial(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"][zone] =
        json!([{"id":"source","card":card,"state":{"entered_from":"cemetery"}}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-083"}]);
    initial
}

fn enter(initial: &Value, zone: &str) -> Game {
    if zone == "hand" || zone == "ex" {
        let mut game = game(initial);
        let CardPlay::Entered(entry) = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("normal play")
        };
        if entry.resume == ResumeStage::CheckTiming {
            game.resume_internal_card().unwrap();
        }
        game
    } else {
        let mut game = with_producer(
            initial,
            Action::Move {
                objects: subjects_in(ZoneKind::Cemetery),
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Field,
                        },
                    },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            },
        );
        activate(&mut game, "P1");
        game.resume_internal_card().unwrap();
        game
    }
}

#[test]
fn bp19_082_free_evolution_requires_the_current_generation_to_enter_from_cemetery() {
    for zone in ["hand", "ex", "cemetery"] {
        let mut game = enter(&initial("BP19-082", zone), zone);
        let before = game.projection(View::Referee).unwrap();
        let decision = json!({"do":"evolve","source":"source","evolve_card":"form","definition":4_u64,"pay":{"pp":0_u8}});
        let result = game.internal_rule_action(&decision).unwrap();
        assert_eq!(
            result.outcome,
            if zone == "cemetery" {
                "resolved"
            } else {
                "cannot-evolve"
            }
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
        if zone == "cemetery" {
            assert_eq!(after["objects"]["source"]["power"], 3_u8);
        } else {
            assert_eq!(
                after, before,
                "a refused free evolution preserves the whole position"
            );
            assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","definition":1_u64,"pay":{"pp":1_u8}})).unwrap().outcome, "resolved", "the unrestricted printed alternative remains usable");
        }
    }
}

#[test]
fn bp19_099_only_nonhand_entry_offers_the_printed_damage_or_healing_choice() {
    for (card, zone, mode) in [
        ("BP19-099", "ex", 1),
        ("PR-524", "cemetery", 2),
        ("BP19-099", "hand", 1),
    ] {
        let mut initial = initial(card, zone);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = enter(&initial, zone);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("fanfare")
        };
        assert_eq!(options[0].card, card);
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        if zone == "hand" {
            resolved(play);
        } else {
            let CardPlay::Awaiting(choice) = play else {
                panic!("choice")
            };
            assert!(matches!(&choice.domain, InputDomain::Modes { .. }));
            resolved(
                game.answer_internal_card(
                    choice.actor,
                    choice.token,
                    Answer::Modes(vec![ModeId(mode)]),
                )
                .unwrap(),
            );
        }
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            if zone != "hand" && mode == 1 {
                18_u8
            } else {
                20
            }
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if zone != "hand" && mode == 2 {
                12_u8
            } else {
                10
            }
        );
    }
}
