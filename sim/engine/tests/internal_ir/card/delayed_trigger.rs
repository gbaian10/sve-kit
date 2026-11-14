use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{InstalledChange, TriggerTiming};

fn tree(delay: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":0},"targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],"do":[delay]});
    tree["cards"] = json!({"unit-follower":{"meta":tree["cards"]["BP08-038"]["meta"].clone(),"abilities":[entry]}});
    tree
}
fn delayed() -> Value {
    json!({"delayed":{"on":{"moved":{"ref":"enemy","from":"field","to":"cemetery"}},"until":"turn","next_only":false,"do":[{"draw":{"who":"self","n":1}}]}})
}

#[test]
fn delayed_callbacks_reject_live_outer_reads_nested_installations_and_other_lifetimes() {
    lowerer().lower(&tree(&delayed())).unwrap();
    let mut hidden = tree(&delayed());
    hidden["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"][0]["from"]["zone"] =
        json!("hand");
    assert!(
        lowerer().lower(&hidden).is_err(),
        "a private hand choice cannot feed an installed watch"
    );

    for (pointer, value) in [
        ("/delayed/until", json!("next_opp_turn")),
        ("/delayed/next_only", json!(true)),
        (
            "/delayed/do",
            json!([{"draw":{"who":"self","n":{"count":{"who":"opp","zone":"hand"}}}}]),
        ),
        ("/delayed/do", json!([delayed()])),
        ("/delayed/targets", json!([])),
        ("/delayed/on/moved/ref", json!("unknown")),
        ("/delayed/on/moved/to", json!("hand")),
    ] {
        let mut delay = delayed();
        if pointer == "/delayed/targets" {
            delay["delayed"]["targets"] = value;
        } else {
            *delay.pointer_mut(pointer).unwrap() = value;
        }
        assert!(
            lowerer().lower(&tree(&delay)).is_err(),
            "{pointer} cannot widen the installed callback"
        );
    }
}

fn life_watch() -> Value {
    let mut life = delayed();
    life["delayed"]["on"] = json!({"life_changed":{"who":"self","direction":"down"}});
    life
}

#[test]
fn installed_life_watch_triggers_on_a_later_own_leader_decrease_this_turn() {
    let mut document = tree(&life_watch());
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        life_watch(), {"damage":{"to":{"leader":"self"},"n":1}}
    ]);
    let lowered = lowerer().lower(&document).unwrap();
    let mut initial = setup(
        1,
        json!([{"id":"d1","card":"unit-follower"}, {"id":"tail","card":"unit-follower"}]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "life-watch",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &lowered["unit-follower"])
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal target");
    };
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    // Players see the installed watch as a life-change trigger, not a departure one.
    assert!(
        game.observe(Viewer::P1)
            .unwrap()
            .players
            .iter()
            .flat_map(|player| &player.effects)
            .any(|effect| effect.change
                == InstalledChange::Delayed {
                    timing: TriggerTiming::LifeChange
                }),
        "the installed watch is public as a life-change trigger"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("installed life occurrence");
    };
    assert_eq!(options.len(), 1, "one life decrease, one occurrence");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d1"]),
        "the callback draws once"
    );
}

#[test]
fn delayed_capture_transfers_reject_wrong_time_private_scope_and_live_target_recapture() {
    let lowered = lowerer().lower(&tree(&delayed())).unwrap();
    let card = lowered["unit-follower"].card();
    for mutation in 0_u8..3 {
        let mut changed = card.clone();
        let ability = &mut changed.abilities[0].node;
        let decision = &ability.play.as_ref().unwrap().parameters[0];
        let ChoiceDomain::Objects {
            binding: selection, ..
        } = &decision.domain
        else {
            panic!("target");
        };
        let selection = *selection;
        let Node::Instruction { instruction } = &mut ability.body.as_mut().unwrap().steps[0].node
        else {
            panic!("delay");
        };
        let Action::Delay { captures, .. } = &mut instruction.action else {
            panic!("delay");
        };
        let Capture::Objects { at, value, into } = &mut captures[0] else {
            panic!("capture");
        };
        match mutation {
            0 => {
                *at = CaptureTime::AtSelection {
                    decision: DecisionId(999),
                }
            }
            1 => into.binding = Binding::new(999, ability.scope),
            _ => {
                *value = Selector::Bound {
                    reference: selection,
                }
            }
        }
        WellFormedCardProgram::check(changed).unwrap_err();
    }
}

#[test]
fn repeated_installation_from_the_same_ability_gets_two_distinct_pending_occurrences() {
    let mut document = tree(&delayed());
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"damage":{"to":"enemy","n":1}}, delayed()
    ]);
    let lowered = lowerer().lower(&document).unwrap();
    let mut initial = setup(
        1,
        json!([
            {"id":"d1","card":"unit-follower"}, {"id":"d2","card":"unit-follower"}, {"id":"tail","card":"unit-follower"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["damage"] = json!(1_u8);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "repeated-delay",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &lowered["unit-follower"])
        .unwrap();
    for _ in 0_u8..2 {
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("formal target");
        };
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
    }
    let observation = game.observe(Viewer::P1).unwrap();
    assert_eq!(observation.pending.len(), 2);
    assert!(
        observation
            .pending
            .iter()
            .all(|pending| pending.timing == TriggerTiming::LeaveField)
    );
    assert!(
        observation
            .players
            .iter()
            .flat_map(|player| &player.effects)
            .any(|effect| effect.change
                == InstalledChange::Delayed {
                    timing: TriggerTiming::LeaveField
                })
    );
    for count in [2, 1] {
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("delayed occurrence");
        };
        assert_eq!(options.len(), count);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
    }
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d1", "d2"])
    );
}

#[test]
fn a_returned_and_replayed_target_is_a_new_generation_in_the_same_turn() {
    let mut document = tree(&delayed());
    let template = document["cards"]["unit-follower"]["abilities"][0].clone();
    let mut entries = Vec::new();
    for (index, (who, steps)) in [
        ("opp", json!([delayed()])),
        ("opp", json!([{"move":{"ref":"enemy","to":"hand"}}])),
        ("self", json!([{"destroy":{"ref":"enemy"}}])),
    ]
    .into_iter()
    .enumerate()
    {
        let mut entry = template.clone();
        entry["line"] = json!(index + 1);
        entry["activate"]["targets"][0]["from"]["who"] = json!(who);
        entry["activate"]["do"] = steps;
        entries.push(entry);
    }
    document["cards"]["unit-follower"]["abilities"] = json!(entries);
    let lowered = lowerer().lower(&document).unwrap();
    let program = &lowered["unit-follower"];
    let ids: Vec<_> = program
        .card()
        .abilities
        .iter()
        .map(|ability| ability.node.id)
        .collect();
    let mut initial = setup(1, json!([{ "id":"tail", "card":"unit-follower" }]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"][0]["owner"] = json!("P1");
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "new-generation-delay",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for id in &ids[..2] {
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", *id, PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("opposing target");
        };
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["f"])
    );
    assert!(matches!(
        game.play_internal_card("f", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", ids[2], PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("own replayed target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("own field");
    };
    assert_eq!(domain.members.len(), 2);
    let handle = domain.members.last().unwrap().handle;
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![handle]),
        )
        .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["f"]));
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["turn"]["active"], "P1");
}

#[test]
fn a_delayed_token_body_keeps_its_plan_when_its_callback_id_matches_a_printed_ability() {
    let mut delay = delayed();
    delay["delayed"]["do"] = json!([{"create":{"token":"unit-spell","to":"ex"}}]);
    let mut document = tree(&delay);
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] =
        json!([delay, {"destroy":{"ref":"enemy"}}]);
    let lowered = lowerer().lower(&document).unwrap();
    let mut definition = lowered["unit-follower"].card().clone();
    let Node::Instruction { instruction } =
        &definition.abilities[0].node.body.as_ref().unwrap().steps[0].node
    else {
        panic!("delay instruction");
    };
    let printed_id = AbilityId(instruction.id.0);
    definition.abilities[0].node.id = printed_id;
    let program = WellFormedCardProgram::check(definition).unwrap();
    let (snapshot, keywords, effects) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-spell" {
                row["faces"][0]["card_type"] = json!("スペル・トークン");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), effects)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "delayed-plan",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", printed_id, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal target");
    };
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!([])
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("delayed callback");
    };
    assert_eq!(options.len(), 1);
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(result.source.definition, printed_id);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 1);
    assert_eq!(view["P2"]["field"], json!([]));
}
