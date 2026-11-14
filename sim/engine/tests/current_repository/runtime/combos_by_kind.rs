#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-by-kind",
    )
    .unwrap()
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

/// Answers every question; the leader-or-follower target picks `pick` (None is the leader).
/// Returns the cards offered by that target question, the leader as None.
fn drive(game: &mut Game, mut step: Option<CardPlay>, pick: Option<&str>) -> Vec<Option<String>> {
    let mut offered = Vec::new();
    for _ in 0_u8..64_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => {
                    if offered.is_empty() {
                        offered = d.members.iter().map(|m| m.card.clone()).collect();
                    }
                    Answer::Targets(
                        d.members
                            .iter()
                            .filter(|m| m.card.as_deref() == pick)
                            .take(1)
                            .map(|m| m.handle)
                            .collect(),
                    )
                }
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Quick { .. } => Answer::Quick(None),
                _ => panic!("unexpected decision: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        if let Some(q) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("expected a pending ability");
            };
            step = Some(
                game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                    .unwrap(),
            );
            continue;
        }
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return offered;
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap(),
        );
    }
    panic!("scenario did not settle");
}

#[test]
fn cp04_041_union_burst_heals_the_leader_or_toughens_another_follower() {
    for pick in [None, Some("BP01-037")] {
        let mut initial = draw_position();
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP04-041"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037","state":{"hp":3_u8,"max_hp":3_u8}}]);
        let mut game = loaded(&initial);
        let step = game.play_internal_card("source", PLAYER).unwrap();
        let offered = drive(&mut game, Some(step), pick);
        assert_eq!(
            offered,
            [None, Some("BP01-037".to_owned())],
            "the leader and another follower, never Maho herself"
        );
        let after = state(&game);
        if pick.is_none() {
            assert_eq!(after["P1"]["leader"]["life"], 11_u8);
            assert_eq!(after["objects"]["ally"]["hp"], 3_u8);
        } else {
            assert_eq!(after["P1"]["leader"]["life"], 10_u8);
            assert_eq!(
                after["objects"]["ally"]["hp"], 4_u8,
                "the follower branch raises its defense instead of any life"
            );
        }
    }
}

#[test]
fn bp21_101_evolving_heals_the_leader_or_toughens_itself_and_burns_the_enemy() {
    for pick in [None, Some("BP21-100")] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP21-100"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"BP21-101"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"hp":5_u8,"max_hp":5_u8}}
        ]);
        let mut game = loaded(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves for its printed cost"
        );
        let offered = drive(&mut game, None, pick);
        assert_eq!(offered, [None, Some("BP21-100".to_owned())], "the leader and the evolved follower");
        let after = state(&game);
        if pick.is_none() {
            assert_eq!(after["P1"]["leader"]["life"], 17_u8);
            assert_eq!(after["objects"]["enemy"]["hp"], 5_u8, "no defense rose");
        } else {
            assert_eq!(after["P1"]["leader"]["life"], 15_u8);
            assert_eq!(
                after["objects"]["enemy"]["hp"], 4_u8,
                "its own defense rose, so every opposing follower takes one damage"
            );
        }
    }
}

#[test]
fn ecp01_057_end_phase_heals_the_leader_or_toughens_an_uma_musume() {
    for pick in [None, Some("ECP01-001")] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"ECP01-057"},
            {"id":"uma","card":"ECP01-001","state":{"hp":2_u8,"max_hp":2_u8}},
            {"id":"plain","card":"BP01-037","state":{"hp":3_u8,"max_hp":3_u8}}
        ]);
        let mut game = loaded(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "the main phase ends"
        );
        let offered = drive(&mut game, None, pick);
        assert_eq!(
            offered,
            [None, Some("ECP01-001".to_owned())],
            "only the leader and an Uma Musume follower"
        );
        let after = state(&game);
        if pick.is_none() {
            assert_eq!(after["P1"]["leader"]["life"], 17_u8);
            assert_eq!(after["objects"]["uma"]["hp"], 2_u8);
        } else {
            assert_eq!(after["P1"]["leader"]["life"], 15_u8);
            assert_eq!(after["objects"]["uma"]["hp"], 4_u8);
        }
        assert_eq!(after["objects"]["plain"]["hp"], 3_u8);
    }
}

#[test]
fn ecp01_057_fields_an_uma_musume_from_hand_that_returns_at_the_end_phase() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"ECP01-057"},
        {"id":"uma","card":"ECP01-001"}
    ]);
    let mut game = loaded(&initial);
    let step = game.play_internal_card("source", PLAYER).unwrap();
    drive(&mut game, Some(step), Some("ECP01-001"));
    let played = state(&game);
    let field = played["P1"]["field"].as_array().unwrap();
    assert!(field.contains(&json!("source")) && field.contains(&json!("uma")));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the main phase ends"
    );
    drive(&mut game, None, None);
    let ended = state(&game);
    assert_eq!(ended["P1"]["hand"], json!(["uma"]), "the granted ability returns it");
    assert_eq!(ended["P1"]["leader"]["life"], 17_u8);
}
