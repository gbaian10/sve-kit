#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these complete printed-card probes."
)]
use super::event_cemetery::finish_checks;
use super::event_combat_life::life;
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn with_changes(initial: &Value, changes: &[(bool, i64)]) -> Game {
    let mut initial = initial.clone();
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut board = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "real-healed-history",
    )
    .unwrap();
    let actions = changes
        .iter()
        .map(|&(own, delta)| {
            life(
                if own {
                    PlayerRef::Controller {}
                } else {
                    PlayerRef::RequireOne {
                        players: Box::new(PlayerSet {
                            relation: PlayerRelation::Opponents,
                            relative_to: PlayerRef::Controller {},
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::One,
                        }),
                    }
                },
                delta,
            )
        })
        .collect();
    board
        .load_internal_card("producer", &producer_steps(actions))
        .unwrap();
    resolved(
        board
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    finish_checks(&mut board);
    board
}
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("the real printed target or material choice");
    };
    question
}
fn ability(number: &str, ordinal: usize) -> AbilityId {
    producer_selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .filter(|entry| entry.node.kind == AbilityKind::Activated)
        .nth(ordinal)
        .unwrap()
        .node
        .id
}

#[test]
fn both_evolved_supporter_prints_use_actual_own_healing_even_after_life_is_lost_again() {
    let cases: &[(&[(bool, i64)], bool)] = &[
        (&[(true, 0)], false),
        (&[(true, 1)], true),
        (&[(true, 1), (true, -1)], true),
        (&[(false, 1)], false),
    ];
    for (normal, form) in [("BP18-110", "BP18-111"), ("BP18-P46", "BP18-P47")] {
        for &(changes, healed) in cases {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":normal}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
            let mut board = with_changes(&initial, changes);
            assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})).unwrap().outcome, "resolved", "the complete normal printing pays its true evolution cost");
            finish_checks(&mut board);
            let pending = board.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = pending.domain else {
                panic!("the real evolve-time ability");
            };
            assert_eq!(options.len(), 1, "one printed evolve-time occurrence");
            assert_eq!(
                options[0].card, form,
                "the selected complete evolved printing supplies the ability"
            );
            let q = awaiting(
                board
                    .answer_internal_card(
                        pending.actor,
                        pending.token,
                        Answer::Pending(options[0].handle),
                    )
                    .unwrap(),
            );
            let InputDomain::Targets(domain) = q.domain else {
                panic!("the printed enemy follower");
            };
            assert_eq!(
                domain.members.len(),
                1,
                "the enemy is the only follower target"
            );
            let result = resolved(
                board
                    .answer_internal_card(
                        q.actor,
                        q.token,
                        Answer::Targets(vec![domain.members[0].handle]),
                    )
                    .unwrap(),
            );
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::Damage(_)))
                    .count(),
                if healed { 2_usize } else { 1_usize }
            );
            finish_checks(&mut board);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["cemetery"],
                json!(["enemy"]),
                "the unconditional two damage kills the real two-life target"
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if healed {
                    18_u8
                } else if changes[0].0 {
                    20_u8
                } else {
                    21_u8
                },
                "only actual own healing enables the leader hit"
            );
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        }
    }
}

#[test]
fn both_orchid_prints_require_own_healing_before_paying_act_and_buff_only_the_chosen_academy_follower()
 {
    for number in ["BP21-099", "BP21-P52"] {
        for healed in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"source","card":number},{"id":"ally","card":"BP21-055"},
                {"id":"ordinary","card":"BP01-171"}
            ]);
            let mut board = with_changes(&initial, &[(true, i64::from(healed))]);
            let before = board.projection(View::Referee).unwrap();
            let play =
                board.activate_internal_card("source", ability(number, 0), PlayerId { seat: 1 });
            if !healed {
                assert!(
                    matches!(play, Err(EngineFailure::IllegalPlay(_))),
                    "the printed usability condition refuses before targets or payment"
                );
                assert_eq!(
                    board.projection(View::Referee).unwrap(),
                    before,
                    "refusal pays no act or resource"
                );
                continue;
            }
            let q = awaiting(play.unwrap());
            let InputDomain::Targets(domain) = q.domain else {
                panic!("the academy follower target");
            };
            assert_eq!(
                domain.members.len(),
                1,
                "the ordinary follower and amulet are ineligible"
            );
            assert_eq!(domain.members[0].card.as_deref(), Some("BP21-055"));
            resolved(
                board
                    .answer_internal_card(
                        q.actor,
                        q.token,
                        Answer::Targets(vec![domain.members[0].handle]),
                    )
                    .unwrap(),
            );
            finish_checks(&mut board);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["acted"], true);
            for stat in ["power", "hp"] {
                assert_eq!(
                    state["objects"]["ally"][stat].as_i64().unwrap(),
                    before["objects"]["ally"][stat].as_i64().unwrap() + 1
                );
                assert_eq!(
                    state["objects"]["ordinary"][stat],
                    before["objects"]["ordinary"][stat]
                );
            }
            assert_eq!(
                state["P1"]["pp"]["current"], 10_u8,
                "the first ability pays act only"
            );
        }
    }
}

#[test]
fn both_orchid_prints_pay_one_pp_and_an_academy_amulet_then_search_a_cost_one_academy_follower_once_per_turn()
 {
    for number in ["BP21-099", "BP21-P52"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"material","card":number},{"id":"reserve","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"wanted","card":"BP21-033"},{"id":"wanted2","card":"BP21-033"},{"id":"wrong_cost","card":"BP21-055"},
            {"id":"wrong_trait","card":"BP01-171"},{"id":"wrong_kind","card":"BP21-012"}
        ]);
        let mut board = with_changes(&initial, &[(true, 0)]);
        let materials = awaiting(
            board
                .activate_internal_card("source", ability(number, 1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::CostMaterials(cost_options) = materials.domain else {
            panic!("the printed hand discard cost");
        };
        assert_eq!(
            cost_options.members.len(),
            2,
            "one matching academy amulet pays now and another remains for the quota counterexample"
        );
        let search = awaiting(
            board
                .answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![cost_options.members[0].handle]),
                )
                .unwrap(),
        );
        let InputDomain::Search(domain) = search.domain else {
            panic!("the real deck search");
        };
        assert_eq!(
            domain.cards.len(),
            2,
            "only the two cards matching cost, academy trait and follower kind are searchable"
        );
        assert!(
            domain.cards.iter().all(|card| card.card == "BP21-033"),
            "wrong cost, trait and kind remain excluded"
        );
        resolved(
            board
                .answer_internal_card(
                    search.actor,
                    search.token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
                )
                .unwrap(),
        );
        finish_checks(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["material"]));
        assert_eq!(state["P1"]["hand"], json!(["reserve", "wanted"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(
            state["objects"]["source"]["acted"], false,
            "the second ability does not pay act"
        );
        assert!(
            matches!(
                board.activate_internal_card("source", ability(number, 1), PlayerId { seat: 1 }),
                Err(EngineFailure::IllegalPlay(_))
            ),
            "the printed once-per-turn quota prevents another activation"
        );
    }
}
