use super::*;

fn diagnostic<T>(result: crate::Result<T>) -> String {
    result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default()
}

fn observations(game: &Game) -> Vec<Value> {
    let mut observations = [View::P1, View::P2, View::Referee]
        .map(|view| {
            game.projection(view)
                .unwrap_or_else(|error| json!({"error":error.to_string()}))
        })
        .to_vec();
    for seat in ["P1", "P2"] {
        for zone in ["hand", "deck", "cemetery", "banish", "field"] {
            observations.push(
                game.query(View::Referee, &format!("{seat}.{zone}_count"))
                    .unwrap()
                    .unwrap(),
            );
        }
    }
    observations
}

fn add_unloaded(initial: &mut Value, seat: &str, zone: &str) -> &'static str {
    let number = if zone == "field" {
        "BP01-173"
    } else {
        "BP06-042"
    };
    let object = json!({"id":"unloaded","card":number,"state":{"acted":true}});
    if zone == "field" {
        initial["players"][seat]["zones"][zone]
            .as_array_mut()
            .unwrap()
            .push(object);
    } else {
        initial["players"][seat]["zones"][zone] = json!([object]);
    }
    number
}

fn assert_unavailable(error: &str, number: &str) {
    assert!(
        error.contains(number),
        "authority must identify the unloaded card"
    );
    assert!(error.contains("holder unloaded generation 0"));
    assert!(error.contains("query "));
    assert!(error.contains("current ability definitions unavailable"));
}

#[test]
fn an_unloaded_keyword_holder_never_uses_a_prototype_or_silenced_empty_set() {
    for seat in ["P1", "P2"] {
        for silenced in [false, true] {
            let mut initial = setup(seat);
            initial["players"][seat]["zones"]["field"] = json!([
                {"id":"unloaded","card":"BP01-173","state":{"silenced":silenced}}
            ]);
            let game = game(&initial, NUMBER, &producer(vec![], None));
            assert_unavailable(&diagnostic(game.projection(View::Referee)), "BP01-173");
            assert_eq!(
                game.query(View::P1, &format!("{seat}.field_count"))
                    .unwrap(),
                Some(json!(1_u8))
            );
        }
    }
}

#[test]
fn unknown_sources_stop_current_keyword_views_attacks_and_end_phase() {
    for seat in ["P1", "P2"] {
        for owner in [seat, opponent(seat)] {
            for zone in ["hand", "deck", "cemetery", "banish", "field"] {
                let mut initial = setup(seat);
                initial["players"][seat]["zones"]["field"] = json!([
                    {"id":"attacker","card":"plain"},
                    {"id":"own-ward","card":NUMBER}
                ]);
                initial["players"][opponent(seat)]["zones"]["field"] =
                    json!([{"id":"opposing-ward","card":NUMBER,"state":{"acted":true}}]);
                let number = add_unloaded(&mut initial, owner, zone);
                let game = game(&initial, NUMBER, &producer(vec![], None));
                let error = diagnostic(game.projection(View::Referee));
                assert_unavailable(&error, number);
                assert!(error.contains("keyword"));
                for view in [View::P1, View::P2] {
                    assert_eq!(
                        diagnostic(game.projection(view)),
                        "unsupported: current projection requires unavailable abilities"
                    );
                }
                for action in [
                    json!({"do":"attack","attacker":"attacker","target":format!("{}.leader",opponent(seat))}),
                    json!({"do":"end-phase"}),
                ] {
                    let mut attempt = game.clone();
                    let before = observations(&attempt);
                    assert_unavailable(&diagnostic(attempt.internal_rule_action(&action)), number);
                    assert_eq!(observations(&attempt), before);
                    assert_eq!(attempt.internal_card_stage(), None);
                    assert!(attempt.internal_quick_choice().unwrap().is_none());
                }
            }
        }
    }
}

#[test]
fn unknown_sources_stop_aura_targets_and_ward_entry_without_starting_a_choice() {
    for seat in ["P1", "P2"] {
        for zone in ["hand", "deck", "cemetery", "banish", "field"] {
            let mut initial = setup(seat);
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"producer","card":"producer"}]);
            initial["players"][opponent(seat)]["zones"]["field"] =
                json!([{"id":"opposing-aura","card":NUMBER}]);
            initial["players"][seat]["zones"]["ex"] = json!([{"id":"incoming","card":NUMBER}]);
            let number = add_unloaded(&mut initial, opponent(seat), zone);
            let program = producer(
                vec![Action::Destroy {
                    objects: Selector::Bound {
                        reference: Binding::new(10, ScopeId(1)),
                    },
                }],
                Some(PlayerRelation::Opponents),
            );
            let game = game(&initial, NUMBER, &program);
            let mut targeting = game.clone();
            let before_targeting = observations(&targeting);
            assert_unavailable(
                &diagnostic(targeting.activate_internal_card(
                    "producer",
                    AbilityId(1),
                    actor(seat),
                )),
                number,
            );
            assert_eq!(observations(&targeting), before_targeting);
            assert_eq!(targeting.internal_card_stage(), None);
            let mut entering = game;
            let before_entering = observations(&entering);
            assert_unavailable(
                &diagnostic(entering.play_internal_card("incoming", actor(seat))),
                number,
            );
            assert_eq!(observations(&entering), before_entering);
            assert_eq!(entering.internal_card_stage(), None);
        }
    }
}
