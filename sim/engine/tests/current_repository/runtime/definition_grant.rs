#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these grant scenarios."
)]
use super::*;

fn target(game: &mut Game, play: CardPlay, card: &str) -> CardPlay {
    let CardPlay::Awaiting(q) = play else {
        panic!("target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("targets")
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap()
        .handle;
    game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![handle]))
        .unwrap()
}

fn pending(game: &mut Game, provider: &str) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending")
    };
    assert_eq!(options.len(), 1, "exactly the granted ability is pending");
    assert_eq!(
        options[0].card, provider,
        "the ability keeps its text provider"
    );
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

fn attack(game: &mut Game, holder: &str) {
    game.internal_rule_action(&json!({"do":"attack","attacker":holder,"target":"P2.leader"}))
        .unwrap();
}

#[test]
fn bp03_083_grants_a_targeted_attack_using_the_recipient_power_after_the_spell_leaves() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP03-083"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037","state":{"power":2_u8}}, {"id":"other","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    resolved(target(&mut game, play, "BP01-037"));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["spell"])
    );
    attack(&mut game, "ally");
    let triggered = pending(&mut game, "BP03-083");
    let activation = resolved(target(&mut game, triggered, "BP01-037"));
    assert_eq!(activation.source.controller, PlayerId { seat: 1 });
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["hp"], 3_u8);
    assert_eq!(view["objects"]["ally"]["power"], 2_u8);
    game.resume_internal_card().unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    attack(&mut game, "other");
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "a different follower did not receive the ability"
    );
}

#[test]
fn cp03_023_all_printings_recompute_the_cemetery_condition_and_grant_self_stats_on_attack() {
    for printing in ["CP03-023", "CP03-SL06"] {
        for count in [14_u8, 15_u8] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":printing}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("dead-{i}"),"card":printing}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            let before = game.projection(View::Referee).unwrap();
            let power = before["objects"]["ally"]["power"].as_i64().unwrap();
            let hp = before["objects"]["ally"]["hp"].as_i64().unwrap();
            attack(&mut game, "ally");
            if count == 15 {
                resolved(pending(&mut game, printing));
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["objects"]["ally"]["power"], power + 1);
                assert_eq!(after["objects"]["ally"]["hp"], hp + 1);
            } else {
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
        }
    }
}

#[test]
fn bp13_119_granted_lastword_survives_the_recipient_departure_and_recovers_then_draws() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP13-119"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: subjects(),
        },
    );
    let play = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    resolved(target(&mut game, play, "BP01-037"));
    game.resume_internal_card().unwrap();
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let lastword = pending(&mut game, "BP13-119");
    resolved(lastword);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
    assert_eq!(view["P1"]["hand"], json!(["drawn"]));
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("ally"))
    );
    assert_eq!(
        view["P1"]["field"],
        json!([]),
        "the recipient left the field before its Lastword resolved"
    );
}
