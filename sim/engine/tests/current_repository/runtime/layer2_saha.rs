#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real Sahaquiel and Israfil scenarios."
)]
use super::*;

fn position(controller: &str, owner: &str, full: bool, target: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-105"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-106"}]);
    if controller == "P2" {
        initial["players"]["P2"]["zones"]["field"] = json!([]);
    }
    initial["players"][controller]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"id":"target","card":target,"owner":owner,"state":{"damage":1_u8,"power":9_u8}}),
        );
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    if full {
        initial["players"][owner]["zones"]["ex"] = json!(
            (0_u8..5)
                .map(|i| json!({"id":format!("ex{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
    }
    initial
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real source must evolve before its evolved ability can trigger"
    );
    for _ in 0_u8..16 {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("pending evolution ability");
        };
        let mut play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => {
                    assert!(
                        domain
                            .members
                            .iter()
                            .all(|m| m.card.as_deref() != Some("BP13-106")),
                        "the source is excluded"
                    );
                    Answer::Targets(vec![domain.members[0].handle])
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected Saha decision: {:?}", question.domain),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
    }
    panic!("Saha scenario did not settle");
}

#[test]
fn bp13_106_returns_only_targets_that_reached_its_controllers_ex() {
    for controller in ["P1", "P2"] {
        for owner in ["P1", "P2"] {
            let mut game = game(&position(controller, owner, false, "BP01-037"));
            evolve(&mut game);
            let state = game.projection(View::Referee).unwrap();
            if owner == "P1" {
                assert!(
                    state["P1"]["field"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("target"))
                );
                assert_eq!(state["objects"]["target"]["generation"], 2_u8);
                assert_eq!(state["objects"]["target"]["acted"], true);
                assert_eq!(state["objects"]["target"]["damage"], 0_u8);
                assert_eq!(state["objects"]["target"]["power"], 5_u8);
                assert_eq!(state["P1"]["ex"], json!([]));
            } else {
                assert_eq!(state["P2"]["ex"], json!(["target"]));
                assert_eq!(state["objects"]["target"]["generation"], 1_u8);
                assert!(
                    !state["P1"]["field"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("target"))
                );
            }
        }
    }
}

#[test]
fn bp13_106_full_destination_does_not_return_the_unmoved_target() {
    for owner in ["P1", "P2"] {
        let mut game = game(&position("P2", owner, true, "BP01-037"));
        evolve(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!(["target"]));
        assert_eq!(state["objects"]["target"]["generation"], 0_u8);
        assert_eq!(state["objects"]["target"]["power"], 9_u8);
        assert_eq!(state[owner]["ex"].as_array().unwrap().len(), 5);
    }
}

#[test]
fn bp13_106_returned_follower_triggers_its_real_fanfare() {
    let mut game = game(&position("P2", "P1", false, "BP15-018"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["objects"]["target"]["acted"], true);
}
