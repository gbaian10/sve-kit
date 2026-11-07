#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Only the printed selection decisions are expected."
)]
use super::*;

#[test]
fn ornate_princess_kiss_keeps_both_modes_and_zero_return_shuffle() {
    shuffle_seeds::princess("BP03-031");
}

#[test]
fn encounter_all_printings_shuffle_before_look_and_field_capacity_caps_choice() {
    for number in ["BP17-066", "PR-455"] {
        for (order, take, occupied) in [
            (vec!["d2", "d0", "d1", "d3"], 2_usize, 0_usize),
            (vec!["d1", "d0", "d2", "d3"], 2, 0),
            (vec!["d2", "d0", "d1", "d3"], 0, 0),
            (vec!["d2", "d0", "d1", "d3"], 2, 4),
            (vec!["d2", "d0", "d1", "d3"], 2, 5),
            (vec![], 2, 0),
        ] {
            encounter(number, &order, take, occupied);
        }
    }
}

fn encounter(number: &str, order: &[&str], take: usize, occupied: usize) {
    // Full current selection verifies the printed aliases with the same catalog as every admitted card.
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":8_u8,"max":8_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..occupied)
            .map(|i| json!({"id":format!("f{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = if order.is_empty() {
        json!([])
    } else {
        json!([
            {"id":"d0","card":"BP01-171"},{"id":"d1","card":"BP03-030"},
            {"id":"d2","card":"BP01-171"},{"id":"d3","card":"BP01-171"}
        ])
    };
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &json!({"shuffles":[{"player":"P1","result":order}]}),
        "shuffle-light-encounter",
    )
    .unwrap();
    let expected_candidates = order.iter().take(2).filter(|id| **id != "d1").count();
    let mut play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(q) = play {
        assert!(
            game.internal_decision(View::P2).is_none(),
            "only the caster chooses the looked-at cards"
        );
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                assert_eq!(
                    d.members.len(),
                    expected_candidates,
                    "every seen follower remains eligible"
                );
                assert!(
                    d.members
                        .iter()
                        .all(|m| m.card.as_deref() == Some("BP01-171")),
                    "the seen spell is not offered as a follower"
                );
                assert_eq!(
                    d.cardinality.minimum,
                    Nat(0),
                    "the up-to choice allows choosing zero"
                );
                let maximum = usize::try_from(d.cardinality.maximum.unwrap().0).unwrap();
                assert_eq!(
                    maximum,
                    2_usize.min(5_usize.saturating_sub(occupied)),
                    "field capacity caps the printed limit of two"
                );
                Answer::Targets(
                    d.members
                        .iter()
                        .take(take.min(maximum))
                        .map(|m| m.handle)
                        .collect(),
                )
            }
            other => panic!("unexpected encounter decision {other:?}"),
        };
        play = game
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap_or_else(|error| {
                panic!(
                    "encounter {number}, order {order:?}, take {take}, occupied {occupied}: {error}"
                )
            });
    }
    let activation = resolved(play);
    assert!(
        activation
            .progress
            .facts
            .iter()
            .any(|f| matches!(f,Fact::Shuffled{player} if *player == PlayerId{seat:1})),
        "shuffle precedes Look even for an empty deck"
    );
    assert_encounter(&game, order, take, occupied);
}

fn assert_encounter(game: &Game, order: &[&str], take: usize, occupied: usize) {
    let seen = order.iter().take(2).copied().collect::<Vec<_>>();
    let eligible = seen.iter().filter(|id| **id != "d1").take(take).count();
    let arrivals = eligible.min(5_usize.saturating_sub(occupied));
    let view = game.projection(View::Referee).unwrap();
    let picked = seen
        .iter()
        .copied()
        .filter(|id| *id != "d1")
        .take(take)
        .collect::<Vec<_>>();
    let arrived = view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| !id.starts_with('f'))
        .collect::<Vec<_>>();
    assert!(
        arrived.iter().all(|id| picked.contains(id)),
        "only selected followers may enter"
    );
    for id in &seen {
        assert_eq!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!(id)),
            !arrived.contains(id),
            "every seen card that did not enter goes to cemetery"
        );
    }

    assert_eq!(
        view["P1"]["pp"]["current"], 0_u8,
        "the printed spell costs eight PP"
    );
    assert_eq!(
        view["P1"]["field"].as_array().unwrap().len(),
        occupied.checked_add(arrivals).unwrap(),
        "entry respects the remaining field capacity"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(order.iter().skip(2).collect::<Vec<_>>()),
        "Look reads the already-shuffled top two cards"
    );
    assert_eq!(
        view["P1"]["cemetery"].as_array().unwrap().len(),
        1_usize
            .checked_add(seen.len().checked_sub(arrivals).unwrap())
            .unwrap(),
        "unarrived seen cards and the spell reach cemetery"
    );
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the resolved spell reaches cemetery"
    );
    if seen.contains(&"d1") {
        assert!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("d1")),
            "a seen spell cannot enter as a follower"
        );
    }
}
