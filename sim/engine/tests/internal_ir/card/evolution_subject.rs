use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

fn tree(from_deck: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut source = tree["cards"]["BP01-041"].clone();
    source["abilities"][0]["activate"] = if from_deck {
        json!({"cost":{"pp":0},"do":[
            {"move":{"ref":"this","to":"cemetery"}},
            {"search":{"sel":{"who":"self","zone":"deck","kind":"follower","name":"recipient"},"n":1,"to":"field","result":"entered"}},
            {"evolve":{"ref":{"follow":"entered"}}}
        ]})
    } else {
        json!({"cost":{"pp":0},"targets":[
            {"bind":"recipient","from":{"who":"self","zone":"field","kind":"follower","name":"recipient"},"n":1}
        ],"do":[{"evolve":{"ref":"recipient"}}]})
    };
    tree["cards"] = json!({"unit-follower":source});
    tree
}

fn programs(from_deck: bool) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(from_deck))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn position(program: &WellFormedCardProgram, from_deck: bool) -> Game {
    position_with_form(program, from_deck, &card(vec![]))
}

fn position_with_form(
    program: &WellFormedCardProgram,
    from_deck: bool,
    form: &WellFormedCardProgram,
) -> Game {
    let (mut snapshot, registry, document) = catalog_documents(&json!("1"));
    for (number, name, kind, power) in [
        ("recipient", "recipient", "フォロワー", "2"),
        ("recipient-form", "recipient", "フォロワー・エボルヴ", "7"),
        ("source-form", "unit-follower", "フォロワー・エボルヴ", "9"),
    ] {
        snapshot.push('\n');
        snapshot.push_str(
            &json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "cost":"1","power":power,"hp":"8","traits":[],"text":null,"sections":[]
            }]})
            .to_string(),
        );
    }
    let definitions = json!({"format":1,"kind":"effect_set","cards":{
        "recipient":{"status":"complete","abilities":[]},
        "recipient-form":{"status":"complete","abilities":[]},
        "source-form":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[
                ("units.yaml".into(), document),
                ("recipients.yaml".into(), definitions),
            ],
        )
        .unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    let zone = if from_deck { "deck" } else { "field" };
    initial["players"]["P1"]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"recipient","card":"recipient"}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"right-form","card":"recipient-form"},{"id":"wrong-form","card":"source-form"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "recipient-evolution",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("recipient-form", form).unwrap();
    for number in ["recipient", "source-form", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[expect(
    clippy::panic,
    reason = "Synthetic fixtures require the declared decision domains."
)]
fn run_case(from_deck: bool, accept: bool) {
    let program = programs(from_deck);
    let mut game = position(&program, from_deck);
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(first) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("recipient selection");
    };
    let answer = if let InputDomain::Search(domain) = &first.domain {
        Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
    } else if let InputDomain::Targets(domain) = &first.domain {
        Answer::Targets(vec![domain.members[0].handle])
    } else {
        panic!("recipient selection domain");
    };
    let CardPlay::Awaiting(form) = game
        .answer_internal_card(first.actor, first.token, answer)
        .unwrap()
    else {
        panic!("optional recipient evolution");
    };
    let InputDomain::Targets(domain) = &form.domain else {
        panic!("evolution resource choice");
    };
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the recipient can decline evolution even when a form exists"
    );
    assert_eq!(
        domain.members.len(),
        1,
        "only the recipient's name determines its evolution resource"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("recipient-form"),
        "the source's form is ineligible"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot inspect private evolution resources"
    );
    let unchanged = format!("{game:?}");
    game.answer_internal_card(form.actor, form.token, Answer::Targets(vec![u64::MAX]))
        .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        unchanged,
        "a forged handle cannot alter either subject or form authority"
    );
    let activation = resolved(
        game.answer_internal_card(
            form.actor,
            form.token,
            Answer::Targets(if accept {
                vec![domain.members[0].handle]
            } else {
                vec![]
            }),
        )
        .unwrap(),
    );
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Evolved(_)))
            .count(),
        usize::from(accept),
        "only accepted recipient evolution emits an event"
    );
    assert_result(&game, &before, from_deck, accept);
}

fn assert_result(game: &Game, before: &Value, from_deck: bool, accept: bool) {
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["recipient"]["power"],
        if accept { 7 } else { 2 },
        "only the recipient acquires evolved characteristics"
    );
    assert_eq!(
        after["objects"]["source"]["power"], 2,
        "the producer is not substituted for the recipient"
    );
    assert_eq!(
        after["objects"]["recipient"]["generation"],
        u8::from(from_deck),
        "receipt following uses the entered generation"
    );
    for resource in ["pp", "ep", "sep"] {
        assert_eq!(
            after["P1"][resource], before["P1"][resource],
            "effect evolution does not pay intrinsic {resource}"
        );
    }
    if from_deck {
        assert_eq!(
            after["P1"]["cemetery"],
            json!(["source"]),
            "a departed producer does not prevent its entered recipient evolving"
        );
    }
}

#[test]
fn selected_own_follower_uses_its_own_name_and_may_decline_effect_evolution() {
    for accept in [false, true] {
        run_case(false, accept);
    }
}

#[test]
fn search_entry_receipt_evolves_the_entered_follower_after_the_producer_left() {
    for accept in [false, true] {
        run_case(true, accept);
    }
}

#[test]
fn a_trigger_resolution_target_keeps_its_authority_through_the_private_form_choice() {
    let mut tree = tree(false);
    let entry = &mut tree["cards"]["unit-follower"]["abilities"][0];
    let mut body = entry.as_object_mut().unwrap().remove("activate").unwrap();
    body.as_object_mut().unwrap().remove("cost");
    let targets = body.as_object_mut().unwrap().remove("targets").unwrap();
    body["do"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"choose":targets[0]}));
    body["on"] = json!({"phase":{"whose":"self","which":"end"}});
    entry["auto"] = body;
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = position(&program, false);
    let before = game.projection(View::Referee).unwrap();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let CardPlay::Awaiting(target) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
        .unwrap()
    else {
        panic!("trigger resolution target");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("public recipient");
    };
    let CardPlay::Awaiting(form) = game
        .answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("private recipient form");
    };
    let InputDomain::Targets(domain) = &form.domain else {
        panic!("evolution resource");
    };
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("recipient-form"),
        "resolution targets use the recipient's own name"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "a trigger's recipient can also decline"
    );
    resolved(
        game.answer_internal_card(
            form.actor,
            form.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_result(&game, &before, false, true);
}

#[test]
fn no_successful_search_entry_skips_the_form_question() {
    let mut tree = tree(true);
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["search"]["sel"]["name"] =
        json!("missing recipient");
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = position(&program, true);
    let CardPlay::Awaiting(search) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("search even when there is no match");
    };
    let InputDomain::Search(domain) = &search.domain else {
        panic!("search domain");
    };
    assert!(
        domain.cards.is_empty(),
        "the nonmatching follower cannot enter"
    );
    let activation = resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![])),
        )
        .unwrap(),
    );
    assert!(
        !activation
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Evolved(_))),
        "no successful member means no evolution"
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["field"],
        json!([]),
        "an empty search does not create a recipient"
    );
    assert_eq!(
        after["P1"]["cemetery"],
        json!(["source"]),
        "the earlier movement remains committed"
    );
    assert_eq!(
        after["P1"]["evolve_deck"],
        json!(["right-form", "wrong-form"]),
        "no form was consumed or queried"
    );
}

#[test]
fn subject_evolution_rejects_other_players_zones_kinds_counts_and_destinations() {
    for from in [
        json!({"who":"opp","zone":"field","kind":"follower"}),
        json!({"who":"self","zone":"cemetery","kind":"follower"}),
        json!({"who":"self","zone":"field","kind":"amulet"}),
        json!({"who":"self","zone":"field"}),
    ] {
        let mut tree = tree(false);
        tree["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"][0]["from"] = from;
        assert!(
            lowerer().lower(&tree).is_err(),
            "only a single own-field follower target grants evolution subject authority"
        );
    }
    for patch in [json!({"count":2}), json!({"into":"recipient"})] {
        let mut tree = tree(false);
        let body = &mut tree["cards"]["unit-follower"]["abilities"][0]["activate"];
        if let Some(count) = patch.get("count") {
            body["targets"][0]["n"] = count.clone();
        }
        if let Some(name) = patch.get("into") {
            body["do"][0]["evolve"]["into"] = name.clone();
        }
        assert!(
            lowerer().lower(&tree).is_err(),
            "multiple subjects and explicit destinations for another subject stay unsupported"
        );
    }
    for to in ["ex", "hand"] {
        let mut tree = tree(true);
        tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["search"]["to"] =
            json!(to);
        assert!(
            lowerer().lower(&tree).is_err(),
            "non-field search output is not an evolution recipient"
        );
    }
    let mut tree = tree(true);
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["search"]["n"] = json!(2);
    assert!(
        lowerer().lower(&tree).is_err(),
        "a multiple-entry result cannot become a single evolution subject"
    );
}

#[test]
fn forged_subject_optionalities_and_form_bindings_fail_well_formedness() {
    let lowered = programs(false);
    for case in 0..4 {
        let mut forged = lowered.card().clone();
        let Node::Control {
            control: Control::Sequence { body: outer },
        } = &mut forged.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("closed evolution projection");
        };
        if case == 0 {
            let Node::Decision { decision } = &mut outer.steps[0].node else {
                panic!("form choice");
            };
            let DecisionKind::ResolutionChoice { may, .. } = &mut decision.kind else {
                panic!("resolution choice");
            };
            *may = Declinable::Required;
        } else {
            let Node::Control {
                control: Control::EachObject { body, .. },
            } = &mut outer.steps[1].node
            else {
                panic!("form iteration");
            };
            let Node::Instruction { instruction } = &mut body.steps[0].node else {
                panic!("evolution instruction");
            };
            let Action::Evolve { subject, form, .. } = &mut instruction.action else {
                panic!("evolution action");
            };
            if case == 1 {
                *subject = ObjectOperand::SourceHolder {};
            }
            if case == 2 {
                *subject = ObjectOperand::Selected {
                    selection: Binding::new(99, ScopeId(999)),
                };
            }
            if case == 3 {
                form.object = ObjectOperand::Bound {
                    binding: Binding::new(99, ScopeId(999)),
                };
            }
        }
        assert!(
            WellFormedCardProgram::check(forged).is_err(),
            "case {case}: the original subject, optionality and inner form binding cannot be substituted"
        );
    }
}

#[test]
fn forged_field_entry_binding_cannot_authorize_the_inner_subject() {
    let lowered = programs(true);
    let mut forged = lowered.card().clone();
    let Node::Control {
        control: Control::EachObject { binding, body, .. },
    } = &mut forged.abilities[0].node.body.as_mut().unwrap().steps[2].node
    else {
        panic!("field entry wrapper");
    };
    *binding = Binding::new(99, body.scope);
    assert!(
        WellFormedCardProgram::check(forged).is_err(),
        "only the actual entry binding can supply the inner recipient"
    );
}

fn chain_programs() -> BTreeMap<String, WellFormedCardProgram> {
    let mut tree = tree(false);
    let abilities = &mut tree["cards"]["unit-follower"]["abilities"];
    let mut intrinsic = abilities[0].clone();
    intrinsic.as_object_mut().unwrap().remove("activate");
    intrinsic["line"] = json!(2);
    intrinsic["evolve"] = json!({"cost":{"pp":1}});
    abilities.as_array_mut().unwrap().push(intrinsic);
    let mut form = tree["cards"]["unit-follower"].clone();
    form["abilities"].as_array_mut().unwrap().truncate(1);
    let entry = &mut form["abilities"][0];
    entry.as_object_mut().unwrap().remove("activate");
    entry["evolve_time"] = json!({"do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]});
    tree["cards"]["recipient-form"] = form;
    lowerer().lower(&tree).unwrap()
}

#[test]
fn the_recipient_evolution_event_triggers_its_form_and_leaves_ordinary_evolution_available() {
    let programs = chain_programs();
    let mut game = position_with_form(
        &programs["unit-follower"],
        false,
        &programs["recipient-form"],
    );
    let CardPlay::Awaiting(target) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("recipient");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("target domain");
    };
    let CardPlay::Awaiting(form) = game
        .answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("form");
    };
    let InputDomain::Targets(domain) = &form.domain else {
        panic!("form domain");
    };
    resolved(
        game.answer_internal_card(
            form.actor,
            form.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("recipient evolution trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "only the evolved recipient observes its own event"
    );
    assert_eq!(
        options[0].card, "recipient-form",
        "the producer did not become the event subject"
    );
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        19,
        "the recipient's true evolution trigger executes"
    );
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"wrong-form","pay":{"pp":1}}),
    )
    .unwrap();
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["source"]["power"], 9,
        "the ordinary evolution quota was not consumed by the effect"
    );
    assert_eq!(
        after["P1"]["pp"]["current"], 1,
        "only ordinary intrinsic evolution paid one PP"
    );
}

#[test]
fn a_recipient_that_left_after_selection_cannot_consume_an_evolution_resource() {
    let mut tree = tree(false);
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"move":{"ref":"recipient","to":"cemetery"}}, {"evolve":{"ref":"recipient"}}
    ]);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = position(&program, false);
    let CardPlay::Awaiting(target) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("recipient");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("target domain");
    };
    let CardPlay::Awaiting(form) = game
        .answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("empty form choice");
    };
    let InputDomain::Targets(domain) = &form.domain else {
        panic!("evolution resources");
    };
    assert!(
        domain.members.is_empty(),
        "the old target generation no longer denotes a field follower"
    );
    resolved(
        game.answer_internal_card(form.actor, form.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["cemetery"],
        json!(["recipient"]),
        "the earlier movement remains committed"
    );
    assert_eq!(
        after["P1"]["evolve_deck"],
        json!(["right-form", "wrong-form"]),
        "a stale recipient cannot consume either resource"
    );
}

fn paid_entry_program() -> WellFormedCardProgram {
    let mut tree = tree(true);
    let entry = &mut tree["cards"]["unit-follower"]["abilities"][0];
    let body = entry.as_object_mut().unwrap().remove("activate").unwrap();
    entry["auto"] = json!({"on":{"phase":{"whose":"self","which":"end"}},"do":[{
        "choice":{"n":1,"options":[
            [{"pay":{"cost":{"steps":[{"pp":1},{"self_to_cemetery":true}]},"do":[body["do"][1],body["do"][2]]}}],
            [{"life":{"who":"self","delta":1}}]
        ]}
    }]});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn a_paid_trigger_can_evolve_its_actual_search_entry_after_paying_and_leaving_the_field() {
    let program = paid_entry_program();
    for pay in [false, true] {
        let mut game = position(&program, true);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let CardPlay::Awaiting(mode) = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
            .unwrap()
        else {
            panic!("resolution payment");
        };
        assert!(
            matches!(mode.domain, InputDomain::Modes { .. }),
            "the trigger declares a mode before its optional payment"
        );
        let CardPlay::Awaiting(payment) = game
            .answer_internal_card(mode.actor, mode.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap()
        else {
            panic!("mode payment");
        };
        assert_eq!(
            payment.domain,
            InputDomain::OptionalCost,
            "the trigger declares payment before searching"
        );
        let result = game
            .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(pay))
            .unwrap();
        if pay {
            let CardPlay::Awaiting(search) = result else {
                panic!("paid search");
            };
            let InputDomain::Search(domain) = &search.domain else {
                panic!("search domain");
            };
            let CardPlay::Awaiting(form) = game
                .answer_internal_card(
                    search.actor,
                    search.token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
                )
                .unwrap()
            else {
                panic!("entered follower evolution");
            };
            let InputDomain::Targets(domain) = &form.domain else {
                panic!("form domain");
            };
            assert_eq!(
                domain.members[0].card.as_deref(),
                Some("recipient-form"),
                "the producer's departure does not change the recipient's name"
            );
            resolved(
                game.answer_internal_card(
                    form.actor,
                    form.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
        } else {
            resolved(result);
        }
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["pp"]["current"],
            if pay { 1 } else { 2 },
            "only the explicit payment spends PP"
        );
        assert_eq!(
            after["objects"]["recipient"]["power"],
            if pay { 7 } else { 2 },
            "declining payment skips search and evolution together"
        );
        assert_eq!(
            after["P1"]["cemetery"],
            if pay { json!(["source"]) } else { json!([]) },
            "the source moves exactly when its cost is paid"
        );
    }
}
