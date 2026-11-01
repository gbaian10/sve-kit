use super::*;

// Only the unadmitted Goblin companion is inert; all three tested cards use their real selection.
fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut record = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|record| record["number"] == "BP01-037").unwrap();
        record["number"] = json!("gate-goblin");
        let face = &mut record["faces"][0];
        face["name"] = json!("Goblin companion");
        face["card_class"] = json!("ニュートラル");
        face["traits"] = json!(["ゴブリン"]);
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        snapshot.push_str(&record.to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "gate-goblin.yaml",
            "format: 1\nkind: effect_set\ncards:\n  gate-goblin:\n    status: complete\n    abilities: []\n"
        ]));
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn position(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "conditional-uses",
    )
    .unwrap();
    game.load_internal_card(
        "gate-goblin",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

fn definition(number: &str) -> AbilityId {
    selected().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn rejected(game: &mut Game, number: &str) {
    let before = format!("{game:?}");
    let error = game
        .activate_internal_card("source", definition(number), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(matches!(error, EngineFailure::IllegalPlay(_)), "{error:?}");
    assert!(
        format!("{game:?}") == before,
        "refusal leaves authority and quota untouched"
    );
}

#[test]
fn bp16_021_requires_ten_own_royal_followers_before_cost_and_limits_each_instance() {
    for number in ["BP16-021", "BP16-SL07", "BP16-U02", "SD07-001"] {
        for (royal, other, allowed) in [(9_usize, 1_usize, false), (10, 0, true)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"source","card":number,"state":{"acted":true}},
                {"id":"second","card":number,"state":{"acted":true}}
            ]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..royal + other).map(|i| json!({"id":format!("grave-{i}"),"card":if i < royal {"BP01-037"} else {"gate-goblin"}})).collect::<Vec<_>>()
            );
            initial["players"]["P2"]["zones"]["cemetery"] =
                json!([{"id":"opponent-royal","card":"BP01-037"}]);
            let mut game = position(&initial);
            if !allowed {
                rejected(&mut game, number);
                continue;
            }
            resolved(
                game.activate_internal_card("source", definition(number), PlayerId { seat: 1 })
                    .unwrap(),
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["objects"]["source"]["acted"], false);
            assert_eq!(view["P1"]["pp"]["current"], 7_u8);
            game.resume_internal_card().unwrap();
            rejected(&mut game, number);
            resolved(
                game.activate_internal_card("second", definition(number), PlayerId { seat: 1 })
                    .unwrap(),
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                4_u8
            );
        }
    }
}

#[test]
fn ecp02_003_requires_three_own_cool_followers_and_free_use_still_consumes_quota() {
    for number in ["ECP02-003", "ECP02-SL03"] {
        for count in [2_usize, 3] {
            let mut initial = draw_position();
            let mut field = vec![json!({"id":"source","card":number})];
            field.extend((1..count).map(|i| json!({"id":format!("cool-{i}"),"card":number})));
            field.push(json!({"id":"wrong-trait","card":"gate-goblin"}));
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"enemy","card":"BP01-037"}, {"id":"opponent-cool","card":number}
            ]);
            let mut game = position(&initial);
            if count == 2 {
                rejected(&mut game, number);
                continue;
            }
            let CardPlay::Awaiting(question) = game
                .activate_internal_card("source", definition(number), PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("target");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("target");
            };
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![
                        domain
                            .members
                            .iter()
                            .find(|member| member.card.as_deref() == Some("BP01-037"))
                            .unwrap()
                            .handle,
                    ]),
                )
                .unwrap(),
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["objects"]["enemy"]["damage"], 1_u8);
            assert_eq!(view["P1"]["pp"]["current"], 10_u8);
            game.resume_internal_card().unwrap();
            rejected(&mut game, number);
        }
    }
}

#[test]
fn bp12_111_checks_goblin_population_from_hand_and_ex_before_targets_or_pp() {
    for zone in ["hand", "ex"] {
        for count in [2_usize, 3] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"spell","card":"BP12-111"}]);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("goblin-{i}"),"card":"gate-goblin"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}, {"id":"other-goblin","card":"gate-goblin"}]);
            let mut game = position(&initial);
            let before = format!("{game:?}");
            let result = game.play_internal_card("spell", PlayerId { seat: 1 });
            if count == 2 {
                assert!(matches!(&result,
                    Err(EngineFailure::Unsupported(message))
                    if message == "internal D1 does not support card play holder, permission or timing"
                ));
                assert!(
                    format!("{game:?}") == before,
                    "forbidden play pays nothing and asks no target"
                );
                continue;
            }
            let CardPlay::Awaiting(question) = result.unwrap() else {
                panic!("target");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("target");
            };
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![
                        domain
                            .members
                            .iter()
                            .find(|member| member.card.as_deref() == Some("BP01-037"))
                            .unwrap()
                            .handle,
                    ]),
                )
                .unwrap(),
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
            assert_eq!(view["P2"]["leader"]["life"], 18_u8);
            assert_eq!(view["P1"]["pp"]["current"], 8_u8);
        }
    }
}

#[test]
fn ecp02_003_fanfare_may_keep_the_top_card_private_or_reveal_it_into_hand() {
    for take in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"ECP02-003"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"ECP02-003"}]);
        let mut game = position(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("fanfare");
        };
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("observed choice");
        };
        assert!(game.internal_decision(View::P2).is_none());
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("observed choice");
        };
        assert_eq!(domain.members.len(), 1);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(if take {
                    vec![domain.members[0].handle]
                } else {
                    vec![]
                }),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["hand"],
            if take { json!(["top"]) } else { json!([]) }
        );
        assert_eq!(
            view["P1"]["deck"],
            if take { json!([]) } else { json!(["top"]) }
        );
        let opponent = game.projection(View::P2).unwrap();
        assert_eq!(
            opponent["known_cards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|known| known["id"] == "top"),
            take
        );
    }
}
