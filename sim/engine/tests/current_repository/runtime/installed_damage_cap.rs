#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::QuickOption;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "installed-damage-cap",
    )
    .unwrap()
}

/// Picks P1's own follower for targets and `modes` for choices; at a Quick window P1 activates
/// `quick` once when offered, every other window is declined.
fn settle(game: &mut Game, mut step: Option<CardPlay>, modes: &[ModeId], quick: &mut Option<&str>) {
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .filter(|m| m.player.seat == 1)
                        .map(|m| m.handle)
                        .take(1)
                        .collect(),
                ),
                InputDomain::Modes { .. } => Answer::Modes(modes.to_vec()),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Quick { options, .. } => {
                    Answer::Quick(offered(options, q.actor, quick))
                }
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
            return;
        };
        let InputDomain::Quick { options, .. } = &q.domain else {
            panic!("expected a Quick window");
        };
        let answer = Answer::Quick(offered(options, q.actor, quick));
        step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
    }
    panic!("scenario did not settle");
}

fn offered(options: &[QuickOption], actor: PlayerId, quick: &mut Option<&str>) -> Option<u64> {
    let handle = options
        .iter()
        .find(|option| actor.seat == 1 && Some(option.card.as_str()) == *quick)
        .map(|option| option.handle);
    if handle.is_some() {
        *quick = None;
    }
    handle
}

fn attack(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved"
    );
    settle(game, None, &[], &mut None);
}

fn damage(game: &Game, id: &str) -> u64 {
    game.projection(View::Referee).unwrap()["objects"][id]["damage"]
        .as_u64()
        .unwrap_or_default()
}

#[test]
fn bp08_024_caps_each_damage_this_turn_and_the_next_opposing_turn() {
    // Q1067: each damage of 4 or more becomes 3; Q1066: the next opposing turn still counts.
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP08-024"},
        {"id":"ally","card":"BP01-037","state":{"power":2_u8,"hp":7_u8,"max_hp":7_u8}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":true,"power":6_u8,"hp":20_u8,"max_hp":20_u8}}
    ]);
    // P2 draws when its turn starts.
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"d1","card":"BP01-037"},{"id":"d2","card":"BP01-037"}]);
    let mut game = game(&initial);
    let ability = selection_with_rules().current["BP08-024"]
        .program
        .card()
        .abilities[0]
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    settle(&mut game, Some(step), &[ModeId(3)], &mut None);
    attack(&mut game, "ally", "enemy");
    assert_eq!(damage(&game, "ally"), 3, "6 becomes 3 this turn");
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(&mut game, None, &[], &mut None);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    attack(&mut game, "enemy", "ally");
    assert_eq!(
        damage(&game, "ally"),
        6,
        "the next opposing turn caps it again"
    );
}

#[test]
fn bp08_024_cap_is_a_gained_ability_lost_with_all_abilities() {
    // The ally gains the cap as an ability; BP11-041 later removes every ability it has.
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP08-024"},
        {"id":"ally","card":"BP01-037","state":{"acted":true,"power":2_u8,"hp":7_u8,"max_hp":7_u8}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"power":6_u8,"hp":20_u8,"max_hp":20_u8}}
    ]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"silence","card":"BP11-041"}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"d1","card":"BP01-037"},{"id":"d2","card":"BP01-037"}]);
    let mut game = game(&initial);
    let ability = selection_with_rules().current["BP08-024"]
        .program
        .card()
        .abilities[0]
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    settle(&mut game, Some(step), &[ModeId(3)], &mut None);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(&mut game, None, &[], &mut None);
    let step = game
        .play_internal_card("silence", PlayerId { seat: 2 })
        .unwrap();
    settle(&mut game, Some(step), &[], &mut None);
    attack(&mut game, "enemy", "ally");
    assert_eq!(
        damage(&game, "ally"),
        6,
        "the lost cap no longer rewrites 6 into 3"
    );
}

#[test]
fn bp05_101_caps_each_leader_damage_this_turn() {
    // Q760: each damage of 5 or more to the own leader becomes 4.
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-101"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"power":6_u8,"hp":20_u8,"max_hp":20_u8}}
    ]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
        .as_u64()
        .unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"P1.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let mut quick = Some("BP05-101");
    settle(&mut game, None, &[], &mut quick);
    assert!(
        quick.is_none(),
        "the defender activates the amulet before damage"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["leader"]["life"].as_u64().unwrap(), before - 4);
    assert_eq!(view["P1"]["cemetery"], json!(["source"]));
}
