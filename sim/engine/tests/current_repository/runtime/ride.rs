#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate printed Ride scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"used","card":"CSD03a-017","face_up":true},
        {"id":"point","card":"CSD03a-017"},
        {"id":"second","card":"CSD03a-017"},
        {"id":"wrong","card":"CSD01-032"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"top","card":"BP01-037"},{"id":"tail","card":"BP01-009"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8,"acted":true}}]);
    initial
}
fn from_position(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ride",
    )
    .unwrap()
}
fn definition(card: &str) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            matches!(
                a.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Ride,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id
}
fn question(game: &mut Game, card: &str) -> DecisionView {
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", definition(card), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Ride payment")
    };
    let InputDomain::ResourceEntryPayment { materials, .. } = &q.domain else {
        panic!("resource choices")
    };
    assert_eq!(
        materials.members.len(),
        2,
        "used DrivePoints and other resource roles are excluded"
    );
    q
}
fn ride(game: &mut Game, card: &str, ep: bool) -> Activation {
    let q = question(game, card);
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: ep,
            },
        )
        .unwrap(),
    )
}
fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("Drive gained pending")
        };
        let mut play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        while let CardPlay::Awaiting(q) = play {
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("Drive gained targets")
            };
            play = game
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(
                        domain
                            .members
                            .first()
                            .map(|m| vec![m.handle])
                            .unwrap_or_default(),
                    ),
                )
                .unwrap();
        }
        resolved(play);
    }
}
#[test]
fn csd03a_006_printings_substitute_ep_gain_drive_and_resolve_the_printed_buff() {
    for card in ["CSD03a-006", "CSD03a-P02"] {
        let mut initial = position(card);
        initial["players"]["P1"]["ep"] = json!(1_u8);
        initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
        let mut game = from_position(&initial);
        let result = ride(&mut game, card, true);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|f| matches!(f, Fact::DriveGained(_)))
                .count(),
            1
        );
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ep"], 0_u8);
        assert_eq!(view["P1"]["drive"], json!(["point"]));
        assert_eq!(view["objects"]["source"]["power"], 5_u8);
        assert_eq!(view["objects"]["source"]["hp"], 5_u8);
        for keyword in ["指定攻撃", "突進", "シングルドライブ", "ドライブ"] {
            assert!(
                view["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(keyword))
            );
        }
        game.activate_internal_card("source", definition(card), PlayerId { seat: 1 })
            .unwrap_err();
    }
}
#[test]
fn csd03b_008_printings_gain_drive_without_evolving_and_can_attack_with_single_drive() {
    for card in ["CSD03b-008", "CSD03b-P05"] {
        let mut game = from_position(&position(card));
        ride(&mut game, card, false);
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["hp"], 8_u8);
        assert_eq!(view["objects"]["source"]["power"], 3_u8);
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        assert_eq!(view["P1"]["evolution"], json!([]));
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        settle(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["deck"],
            json!(["tail", "top"])
        );
    }
}
#[test]
fn cp03_092_printings_resolve_damage_buff_then_mill_after_acquiring_drive() {
    for card in ["CP03-092", "CP03-P61"] {
        let mut game = from_position(&position(card));
        ride(&mut game, card, false);
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["hp"], 8_u8);
        assert_eq!(view["objects"]["source"]["power"], 3_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["top"]));
    }
}
#[test]
fn ride_rejects_foreign_resource_handles_actors_and_unpayable_ep_atomically() {
    let mut game = from_position(&position("CSD03a-006"));
    let q = question(&mut game, "CSD03a-006");
    let before = game.projection(View::Referee).unwrap();
    for (actor, materials, use_ep) in [
        (PlayerId { seat: 2 }, vec![0], false),
        (q.actor, vec![99], false),
        (q.actor, vec![0, 0], false),
        (q.actor, vec![0], true),
    ] {
        game.answer_internal_card(
            actor,
            q.token,
            Answer::ResourceEntryPayment { materials, use_ep },
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: false,
            },
        )
        .unwrap(),
    );
}

#[test]
fn ride_and_evolution_share_the_same_turn_quota_in_both_orders() {
    let mut initial = position("CSD03a-006");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"evolving","card":"BP02-118"}));
    initial["players"]["P1"]["zones"]["evolve_deck"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"form","card":"BP02-119"}));
    let evolution =
        json!({"do":"evolve","source":"evolving","evolve_card":"form","pay":{"pp":3_u8}});
    let mut ride_first = from_position(&initial);
    ride(&mut ride_first, "CSD03a-006", false);
    settle(&mut ride_first);
    assert_eq!(
        ride_first.internal_rule_action(&evolution).unwrap().outcome,
        "cannot-evolve"
    );
    let mut evolve_first = from_position(&initial);
    assert_eq!(
        evolve_first
            .internal_rule_action(&evolution)
            .unwrap()
            .outcome,
        "resolved"
    );
    settle(&mut evolve_first);
    evolve_first
        .activate_internal_card("source", definition("CSD03a-006"), PlayerId { seat: 1 })
        .unwrap_err();
}

#[test]
fn leaving_the_field_releases_the_drive_point_and_returns_it_face_up() {
    let mut initial = position("CSD03a-006");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"offering","card":"BP01-140"}]);
    let mut game = from_position(&initial);
    ride(&mut game, "CSD03a-006", false);
    settle(&mut game);
    let CardPlay::Awaiting(q) = game
        .play_internal_card("offering", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("offering target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("offering target domain")
    };
    let source = domain
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some("CSD03a-006"))
        .unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![source.handle]))
            .unwrap(),
    );
    settle(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["drive"], json!([]));
    assert!(
        view["P1"]["evolve_deck"]
            .as_array()
            .unwrap()
            .contains(&json!("point"))
    );
    assert_eq!(view["objects"]["point"]["face_up"], true);
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}
