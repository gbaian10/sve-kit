#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate same-name token scenarios."
)]
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::{Answer, InputDomain};

fn tree(head: &Value, number: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry
        .as_object_mut()
        .unwrap()
        .extend(head.as_object().unwrap().clone());
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({number:{"meta":meta,"abilities":[entry]}});
    tree
}
fn program(head: &Value, number: &str) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(head, number))
        .unwrap()
        .remove(number)
        .unwrap()
}
fn copied(zone: &str) -> Value {
    json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"chosen","from":{"who":"self","zone":zone,"kind":"follower","token":true},"n":1}],"do":[{"create":{"token":{"same_name_as":"chosen"},"to":zone}}]}})
}
fn initial(zone: &str) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"caster","card":"unit-amulet"}]);
    let chosen = json!({"id":"chosen","card":"unit-follower","state":{"power":9,"hp":9,"max_hp":9,"counters":{"mark":4}}});
    if zone == "field" {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(chosen);
    } else {
        initial["players"]["P1"]["zones"]["ex"] = json!([chosen]);
    }
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial
}
fn game(initial: &Value, admitted_template: bool) -> Game {
    variant_game(initial, admitted_template, "1", true)
}
fn variant_game(initial: &Value, admitted_template: bool, cost: &str, token: bool) -> Game {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["card_type"] = json!(if token {
                    "フォロワー・トークン"
                } else {
                    "フォロワー"
                });
                row["faces"][0]["cost"] = json!(cost);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut second = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    second["number"] = json!("unit-second");
    second["faces"][0]["name"] = json!("second token");
    second["faces"][0]["card_type"] = json!("フォロワー・トークン");
    second["faces"][0]["cost"] = json!("1");
    rows.push(second);
    if !admitted_template {
        let mut representative = rows
            .iter()
            .find(|row| row["number"] == "unit-follower")
            .unwrap()
            .clone();
        representative["number"] = json!("unit-template");
        rows.push(representative);
    }
    let snapshot = rows
        .into_iter()
        .map(|row| row.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-second"] =
        json!({"status":"complete","review":"synthetic","abilities":[]});
    if !admitted_template {
        document["cards"]["unit-template"] =
            json!({"status":"complete","review":"synthetic","token_template":true,"abilities":[]});
    }
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &keywords,
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    );
    let mut game = Game::new(catalog, initial, &Value::Null, &Value::Null, "same-name").unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-second", &card(vec![]))
        .unwrap();
    game
}
fn choose(game: &mut Game, play: CardPlay) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("target");
    };
    let InputDomain::Targets(domain) = question.domain else {
        panic!("target domain");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "one public token is the sole legal target"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![domain.members[0].handle]),
    )
    .unwrap()
}
#[test]
fn copies_selected_field_and_ex_tokens_from_their_printed_template_without_counters_or_buffs() {
    for zone in ["field", "ex"] {
        let mut game = game(&initial(zone), true);
        game.load_internal_card("unit-amulet", &program(&copied(zone), "unit-amulet"))
            .unwrap();
        let play = game
            .activate_internal_card("caster", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        resolved(choose(&mut game, play));
        game.resume_internal_card().unwrap();
        let view = game.projection(View::Referee).unwrap();
        let created = view["P1"][zone]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|id| !["caster", "chosen"].contains(id))
            .unwrap();
        assert_eq!(view["objects"][created]["power"], 2);
        assert_eq!(view["objects"][created]["max_hp"], 3);
        assert_eq!(view["objects"][created]["counters"], json!({}));
        assert_eq!(view["objects"]["chosen"]["power"], 9);
        assert_eq!(view["objects"]["chosen"]["counters"]["mark"], 4);
    }
}
#[test]
fn dynamic_copy_keeps_the_existing_field_capacity_limit() {
    let mut initial = initial("field");
    for n in 0_u8..3 {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("filler{n}"),"card":"unit-amulet"}));
    }
    let mut game = game(&initial, true);
    game.load_internal_card("unit-amulet", &program(&copied("field"), "unit-amulet"))
        .unwrap();
    let before = game.projection(View::Referee).unwrap();
    let play = game
        .activate_internal_card("caster", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    resolved(choose(&mut game, play));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        before["P1"]["field"]
    );
}
#[test]
fn departed_and_erased_tokens_keep_only_the_name_needed_to_create_a_new_printed_token() {
    let observer = json!({"auto":{"per_turn":1,"on":{"left_field":{"who":"self","sel":{"token":true,"kind":"follower","orig_cost_eq":1}}},"do":[{"create":{"token":{"same_name_as":"it"},"to":"field"}}]}});
    let mut initial = initial("field");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"unit-spell"}]);
    let mut game = game(&initial, true);
    game.load_internal_card("unit-amulet", &program(&observer, "unit-amulet"))
        .unwrap();
    let producer = program(
        &json!({"activate":{"cost":{"pp":0},"do":[{"move":{"ref":{"who":"self","zone":"field","kind":"follower"},"to":"cemetery"}}]}}),
        "unit-spell",
    );
    let steps = producer.card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .steps
        .clone();
    game.load_internal_card("unit-spell", &card(vec![play::spell(steps)]))
        .unwrap();
    resolved(
        game.play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["chosen"].is_null(),
        "the original token is erased before the trigger resolves"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("pending");
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let view = game.projection(View::Referee).unwrap();
    let created = view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| *id != "caster")
        .unwrap();
    assert_eq!(view["objects"][created]["power"], 2);
    assert_eq!(view["objects"][created]["counters"], json!({}));
}
#[test]
fn copied_name_syntax_rejects_unknown_keys_counts_and_unbound_or_unhandled_event_subjects() {
    for token in [
        json!({"same_name_as":"chosen","unknown":true}),
        json!({"same_name_as":"missing"}),
    ] {
        let mut head = copied("field");
        head["activate"]["do"][0]["create"]["token"] = token;
        assert!(
            lowerer().lower(&tree(&head, "unit-amulet")).is_err(),
            "the copied name must have a known producer"
        );
    }
    let mut counted = copied("field");
    counted["activate"]["do"][0]["create"]["n"] = json!(2);
    assert!(
        lowerer().lower(&tree(&counted, "unit-amulet")).is_err(),
        "this slice creates one named token"
    );
    for on in [
        json!({"played":{"who":"self","sel":{"kind":"spell","orig_cost_eq":1}}}),
        json!({"enter_field":{"sel":{"orig_cost_eq":1}}}),
    ] {
        let head = json!({"auto":{"on":on,"do":[{"create":{"token":{"same_name_as":"it"},"to":"field"}}]}});
        assert!(
            lowerer().lower(&tree(&head, "unit-amulet")).is_err(),
            "name LKI and original-cost filtering stay scoped to departure"
        );
    }
}

#[test]
fn runtime_rejects_an_unadmitted_dynamic_template_without_creating_any_objects() {
    let mut game = game(&initial("field"), false);
    game.load_internal_card("unit-amulet", &program(&copied("field"), "unit-amulet"))
        .unwrap();
    let before = game.projection(View::Referee).unwrap();
    let play = game
        .activate_internal_card("caster", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("target");
    };
    let InputDomain::Targets(domain) = question.domain else {
        panic!("target domain");
    };
    let error = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("admitted whole card") || error.to_string().contains("loaded")
    );
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
#[test]
fn captured_names_cannot_read_a_private_or_unrelated_object_view() {
    let mut private = copied("field");
    private["activate"]["targets"][0]["from"]["zone"] = json!("hand");
    lowerer().lower(&tree(&private, "unit-amulet")).unwrap_err();
    let mut raw = program(&copied("field"), "unit-amulet").card().clone();
    let Capture::Name { value, .. } =
        &mut raw.abilities[0].node.play.as_mut().unwrap().parameters[0].captures[0]
    else {
        panic!("name capture");
    };
    *value = Expr::Compute(Box::new(NameOperation::UniqueName {
        object: ObjectView::Current {
            object: Box::new(ObjectOperand::SourceHolder {}),
        },
    }));
    WellFormedCardProgram::check(raw).unwrap_err();
}
fn observer() -> Value {
    json!({"auto":{"per_turn":1,"on":{"left_field":{"who":"self","sel":{"token":true,"kind":"follower","orig_cost_eq":1}}},"do":[{"create":{"token":{"same_name_as":"it"},"to":"field"}}]}})
}
fn departure(game: &mut Game, seat: u32) -> CardPlay {
    let lowered = program(
        &json!({"activate":{"cost":{"pp":0},"do":[{"move":{"ref":{"who":"all","zone":"field","kind":"follower"},"to":"cemetery"}}]}}),
        "unit-spell",
    );
    let steps = lowered.card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .steps
        .clone();
    game.load_internal_card("unit-spell", &card(vec![play::spell(steps)]))
        .unwrap();
    game.play_internal_card("spell", PlayerId { seat }).unwrap()
}
#[test]
fn departure_uses_original_cost_and_token_kind_instead_of_current_cost() {
    for (original, effective, is_token, expected) in [
        ("1", 9, true, true),
        ("2", 1, true, false),
        ("1", 1, false, false),
    ] {
        let mut initial = initial("field");
        initial["players"]["P1"]["zones"]["field"][1]["state"]["cost"] = json!(effective);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"unit-spell"}]);
        let mut game = variant_game(&initial, true, original, is_token);
        game.load_internal_card("unit-amulet", &program(&observer(), "unit-amulet"))
            .unwrap();
        resolved(departure(&mut game, 1));
        game.resume_internal_card().unwrap();
        assert_eq!(game.internal_pending_choice().unwrap().is_some(), expected);
    }
}
#[test]
fn both_hosts_trigger_during_the_opponents_turn_with_separate_once_per_turn_quotas() {
    let mut initial = initial("field");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"caster2","card":"unit-amulet"}));
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"spell","card":"unit-spell"}]);
    let mut game = game(&initial, true);
    game.load_internal_card("unit-amulet", &program(&observer(), "unit-amulet"))
        .unwrap();
    resolved(departure(&mut game, 2));
    game.resume_internal_card().unwrap();
    for remaining in [2, 1] {
        let question = game.internal_pending_choice().unwrap().unwrap();
        assert_eq!(question.actor, PlayerId { seat: 1 });
        let InputDomain::Pending(options) = question.domain else {
            panic!("pending");
        };
        assert_eq!(options.len(), remaining);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
    }
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn phase_trigger_copies_a_selected_name_without_requesting_departure_information() {
    let mut head = copied("field");
    let mut body = head["activate"].take();
    body.as_object_mut().unwrap().remove("cost");
    body["on"] = json!({"phase":{"which":"end","whose":"self"}});
    head = json!({"auto":body});
    let mut game = game(&initial("field"), true);
    game.load_internal_card("unit-amulet", &program(&head, "unit-amulet"))
        .unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("pending");
    };
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    resolved(choose(&mut game, play));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn simultaneous_departures_offer_different_names_but_admit_only_one_per_holder_and_turn() {
    let mut choices = BTreeSet::new();
    for choice in [0_usize, 1] {
        let mut initial = initial("field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second","card":"unit-second"}));
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"spell","card":"unit-spell"},{"id":"spell2","card":"unit-spell"}]);
        let mut game = game(&initial, true);
        game.load_internal_card("unit-amulet", &program(&observer(), "unit-amulet"))
            .unwrap();
        let CardPlay::Awaiting(question) = departure(&mut game, 1) else {
            panic!("quota before movement commits");
        };
        let InputDomain::TriggerQuota(domain) = question.domain else {
            panic!("quota domain");
        };
        assert_eq!(domain.candidates.len(), 2);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::TriggerQuota(domain.candidates[choice].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = question.domain else {
            panic!("pending");
        };
        assert_eq!(options.len(), 1);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        let view = game.projection(View::Referee).unwrap();
        let created = view["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "caster")
            .unwrap();
        choices.insert(
            view["objects"][created]["name"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        resolved(
            game.play_internal_card("spell2", PlayerId { seat: 1 })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the selected occurrence consumes this holder's turn quota"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"],
            json!(["caster"])
        );
    }
    assert_eq!(
        choices,
        BTreeSet::from(["unit-follower".to_owned(), "second token".to_owned()])
    );
}
