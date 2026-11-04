#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate complete printed-card probes."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn rules(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "real-prevention-controller",
    )
    .unwrap()
}
fn cast(board: &mut Game) -> Activation {
    let mut play = board
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::DamageReplacements(options) = question.domain else {
            panic!("only the affected player's replacement order is expected");
        };
        assert_eq!(
            question.actor,
            PlayerId { seat: 2 },
            "the affected player orders these opposing hits"
        );
        play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Order(options.iter().map(|option| option.handle).collect()),
            )
            .unwrap();
    }
    let result = resolved(play);
    finish_checks(board);
    result
}

fn amounts(result: &Activation) -> Vec<u64> {
    result
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit.amount.0.get())
            } else {
                None
            }
        })
        .collect()
}
fn position(number: &str, own: bool, spell: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":spell}]);
    let side = if own { "P1" } else { "P2" };
    initial["players"][side]["zones"]["field"] = json!([{"id":"shield","card":number}]);
    initial
}

#[test]
fn both_lazarus_prints_prevent_opponent_spells_but_take_their_own_spell_damage() {
    for number in ["CP03-010", "CP03-P06"] {
        let mut opposing = rules(&position(number, false, "BP01-064"));
        let result = cast(&mut opposing);
        assert!(
            amounts(&result).is_empty(),
            "the real opposing four-damage spell is prevented"
        );
        assert_eq!(
            opposing.projection(View::Referee).unwrap()["P2"]["field"],
            json!(["shield"])
        );
        let mut own = rules(&position(number, true, "BP01-089"));
        let own_result = cast(&mut own);
        assert_eq!(amounts(&own_result), vec![5]);
        assert_eq!(
            own.projection(View::Referee).unwrap()["P1"]["cemetery"],
            json!(["spell", "shield"])
        );
    }
}

#[test]
fn riza_reduces_each_opposing_spell_hit_and_two_providers_stack_to_zero() {
    for (count, spell, expected) in [(1, "BP01-064", 3), (2, "BP01-064", 2), (2, "BP01-180", 0)] {
        let mut initial = position("BP02-004", false, spell);
        let mut field =
            vec![json!({"id":"ordinary","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}})];
        field.extend((0..count).map(|n| json!({"id":format!("riza{n}"),"card":"BP02-004"})));
        initial["players"]["P2"]["zones"]["field"] = json!(field);
        let mut board = rules(&initial);
        let result = cast(&mut board);
        let hits = amounts(&result);
        assert_eq!(
            hits,
            if expected == 0 {
                vec![]
            } else {
                vec![expected; count + 1]
            }
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["ordinary"]["damage"],
            expected
        );
    }
}

#[test]
fn riza_does_not_reduce_own_spells_or_damage_to_opposing_followers() {
    let mut initial = position("BP02-004", true, "BP01-089");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut board = rules(&initial);
    assert_eq!(amounts(&cast(&mut board)), vec![5, 5]);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("shield"))
    );
}

#[test]
fn real_static_providers_do_not_prevent_combat_or_attack_damage() {
    for number in ["CP03-010", "CP03-P06", "BP02-004"] {
        for target in ["shield", "P2.leader"] {
            let mut initial = position(number, false, "BP01-064");
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"attacker","card":"BP01-171"}]);
            let mut board = rules(&initial);
            board
                .internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":target}))
                .unwrap();
            damage_prevent::pass_quick(&mut board);
            finish_checks(&mut board);
            let state = board.projection(View::Referee).unwrap();
            if target == "P2.leader" {
                assert_eq!(state["P2"]["leader"]["life"], 18_u8);
            } else if number == "BP02-004" {
                assert_eq!(state["objects"]["shield"]["damage"], 2_u8);
            } else {
                assert_eq!(
                    state["P2"]["cemetery"],
                    json!(["shield"]),
                    "the printed two-life Lazarus takes the real two-power combat hit"
                );
            }
        }
    }
}

fn damage_others() -> Action {
    Action::Damage {
        recipients: RecipientSet::Objects {
            objects: Selector::Difference {
                input: Box::new(subjects()),
                except: Box::new(Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                }),
            },
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(2)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

#[test]
fn actual_departure_and_silence_remove_rizas_live_reduction() {
    for departed in [false, true] {
        let mut initial = position("BP02-004", false, "BP01-064");
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ordinary","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}));
        if !departed {
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"silenced":true});
        }
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
        let mut board = Game::from_selected_effects(
            producer_selection().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "live-riza-departure",
        )
        .unwrap();
        let actions = if departed {
            vec![
                Action::Destroy {
                    objects: damage_prevent::named_field("エルフの少女・リザ", 51),
                },
                damage_others(),
            ]
        } else {
            vec![damage_others()]
        };
        board
            .load_internal_card("producer", &producer_steps(actions))
            .unwrap();
        let result = resolved(
            board
                .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        assert!(amounts(&result).iter().all(|amount| *amount == 2));
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["ordinary"]["damage"],
            2_u8
        );
        if departed {
            assert_eq!(
                board.projection(View::Referee).unwrap()["P2"]["cemetery"],
                json!(["shield"])
            );
        }
    }
}
