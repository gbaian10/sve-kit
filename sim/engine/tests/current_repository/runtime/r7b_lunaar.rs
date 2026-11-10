#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only the printed evolution, Look and Ward entry decisions may occur."
)]
use super::event_cemetery::finish_checks;
use super::*;

#[test]
fn r7b_lunaar_all_prints_evolve_place_a_low_cost_ward_follower_and_heal() {
    for number in ["BP13-092", "BP22-R15", "PR-416"] {
        assert!(
            selection_with_rules().current.contains_key(number),
            "Every JP printing must be admitted."
        );
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("ビショップ");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-091"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"ward","card":"BP13-091"},{"id":"high","card":"BP18-042"},
            {"id":"spell","card":"BP01-024"},{"id":"plain","card":"BP01-043"},
            {"id":"tail","card":"BP13-091"}
        ]);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "r7b-lunaar",
        )
        .unwrap();
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        finish_checks(&mut game);
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution pending")
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, number);
        let mut step = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let mut selected = false;
        let mut ordered = false;
        while let CardPlay::Awaiting(q) = step {
            let answer = match &q.domain {
                InputDomain::Targets(domain) => {
                    assert!(!selected);
                    assert_eq!(
                        domain.members.len(),
                        2,
                        "only the two observed low-cost followers"
                    );
                    assert_eq!(domain.cardinality.minimum, Nat(0));
                    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
                    assert!(
                        game.internal_decision(View::P2).is_none(),
                        "Only the chooser sees the Look selection."
                    );
                    let recipient = domain
                        .members
                        .iter()
                        .find(|m| m.card.as_deref() == Some("BP13-091"))
                        .unwrap();
                    selected = true;
                    Answer::Targets(vec![recipient.handle])
                }
                InputDomain::Order(cards) => {
                    assert_eq!(cards.len(), 3);
                    ordered = true;
                    Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("printed evolution sequence"),
            };
            step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        resolved(step);
        finish_checks(&mut game);
        assert!(
            selected && ordered,
            "Both printed choice procedures must complete."
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 12_u8);
        assert_eq!(view["P1"]["field"], json!(["source", "ward"]));
        assert_eq!(
            view["P1"]["deck"],
            json!(["tail", "plain", "spell", "high"])
        );
        assert_eq!(view["objects"]["source"]["evolved"], true);
        assert_eq!(view["objects"]["ward"]["generation"], 1_u8);
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "The evolution leaves no unresolved ability."
        );
    }
}
