use super::event_cemetery::{finish_checks, move_material, position, prepared, resolve_event};
use super::*;

#[expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the native draw and discard scenario."
)]
fn resolve_loot(game: &mut Game, card: &str) {
    let occurrence = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &occurrence.domain else {
        panic!("native opponent burial occurrence")
    };
    assert_eq!(
        options.len(),
        1,
        "two successful follower exits share one per-turn allowance"
    );
    assert_eq!(
        options[0].card, card,
        "the native printing owns the admitted occurrence"
    );
    let step = game
        .answer_internal_card(
            occurrence.actor,
            occurrence.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(discard) = step else {
        panic!("native private discard")
    };
    let InputDomain::Targets(domain) = &discard.domain else {
        panic!("native private hand domain")
    };
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["hand"],
        json!(["draw-one"]),
        "the actual draw precedes the discard choice"
    );
    assert_eq!(
        before["P1"]["deck"],
        json!(["draw-two"]),
        "the remaining card stays in the deck before discarding"
    );
    assert_eq!(
        domain.members.len(),
        1,
        "only the newly drawn hand card can be discarded"
    );
    resolved(
        game.answer_internal_card(
            discard.actor,
            discard.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    finish_checks(game);
}

#[test]
fn cp03_081_printings_draw_then_discard_only_once_for_two_opposing_follower_exits_on_their_turn() {
    for card in ["CP03-081", "PR-280"] {
        let mut initial = position(card, "P1", "movement-unit", "P2", "field");
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second","card":"movement-unit"}));
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw-one","card":"movement-other"},{"id":"draw-two","card":"movement-other"}]);
        let mut game = prepared(
            &initial,
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        let moved = game
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(quota) = moved else {
            panic!("simultaneous once-per-turn choice")
        };
        let InputDomain::TriggerQuota(domain) = &quota.domain else {
            panic!("native trigger quota domain")
        };
        assert_eq!(
            domain.candidates.len(),
            2,
            "each of the two successful exits offers its occurrence"
        );
        assert!(
            domain
                .candidates
                .iter()
                .all(|candidate| candidate.card == card),
            "only the real observer competes for its allowance"
        );
        resolved(
            game.answer_internal_card(
                quota.actor,
                quota.token,
                Answer::TriggerQuota(domain.candidates[0].handle),
            )
            .unwrap(),
        );
        finish_checks(&mut game);
        resolve_loot(&mut game, card);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["hand"], json!([]));
        assert_eq!(
            after["P1"]["cemetery"],
            json!(["draw-one"]),
            "one card is discarded for the two enemy events"
        );
        assert_eq!(
            after["P1"]["deck"],
            json!(["draw-two"]),
            "only one card was drawn"
        );
        for id in ["material", "second"] {
            assert!(
                after["P2"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(id)),
                "both opposing followers actually reached the cemetery"
            );
        }
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the second header cannot consume a second allowance"
        );
    }
}

#[test]
fn cp03_081_counterexamples_preserve_the_deck_without_a_burial_occurrence() {
    for (active, side, material, to) in [
        ("P1", "P1", "movement-unit", OwnerZone::Cemetery),
        ("P2", "P2", "movement-unit", OwnerZone::Cemetery),
        ("P1", "P2", "movement-amulet", OwnerZone::Cemetery),
        ("P1", "P2", "movement-unit", OwnerZone::Hand),
    ] {
        let mut initial = position("CP03-081", active, material, side, "field");
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"untouched","card":"movement-other"}]);
        let mut game = prepared(&initial, move_material(ZoneKind::Field, to));
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "CP03-081", 0);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["deck"],
            json!(["untouched"]),
            "the counterexample never draws or discards"
        );
    }
}

#[test]
fn cp03_081_printings_declare_their_fanfare_target_before_the_awakened_damage_guard() {
    for card in ["CP03-081", "PR-280"] {
        for maximum in [6_u8, 7_u8] {
            let mut initial = draw_position();
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            let mut game = Game::from_selected_effects(
                selection_with_rules().clone(),
                &initial,
                &Value::Null,
                &Value::Null,
                "cp03-native-title-fanfare",
            )
            .unwrap();
            assert!(
                matches!(
                    game.play_internal_card("source", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "the real printed follower enters before its Fanfare"
            );
            finish_checks(&mut game);
            let occurrence = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &occurrence.domain else {
                panic!("native Fanfare")
            };
            assert_eq!(options.len(), 1);
            let step = game
                .answer_internal_card(
                    occurrence.actor,
                    occurrence.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            let CardPlay::Awaiting(declaration) = step else {
                panic!("formal Fanfare target below or above awakening")
            };
            let InputDomain::Targets(domain) = &declaration.domain else {
                panic!("formal opposing follower target")
            };
            let enemy = domain
                .members
                .iter()
                .find(|m| m.card.as_deref() == Some("BP01-037"))
                .unwrap();
            resolved(
                game.answer_internal_card(
                    declaration.actor,
                    declaration.token,
                    Answer::Targets(vec![enemy.handle]),
                )
                .unwrap(),
            );
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if maximum >= 7 { 2_u8 } else { 0_u8 },
                "maximum PP guards the damage after declaring the target"
            );
        }
    }
}
