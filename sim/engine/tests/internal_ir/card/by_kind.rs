use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

/// A spell choosing `target` and running `steps`, lowered through the author pipeline.
fn spell(target: &Value, steps: &Value) -> Result<WellFormedCardProgram, lowering::LoweringError> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["spell"] = json!({"targets":[target],"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-spell":{"meta":meta,"abilities":[entry]}});
    lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
        .map(|mut cards| cards.remove("unit-spell").unwrap())
}

fn leader_or(count: &str) -> Value {
    json!({"bind":"t","from":{"who":"self","zone":"field","kind":"follower","leader_or":true},count:1})
}

fn by_kind(name: Option<&str>) -> Value {
    let item = name.unwrap_or("item");
    let mut step = json!({"ref":"t","leader":[{"life":{"who":item,"delta":2}}],
        "follower":[{"buff":{"on":item,"hp":2}}]});
    if let Some(name) = name {
        step["as"] = json!(name);
    }
    json!({"by_kind":step})
}

#[test]
fn by_kind_narrows_a_single_leader_or_follower_target_under_item_or_its_own_name() {
    for name in [None, Some("chosen")] {
        let program = spell(&leader_or("n"), &json!([by_kind(name)])).unwrap();
        assert!(
            program.card().abilities[0]
                .node
                .body
                .as_ref()
                .unwrap()
                .steps
                .iter()
                .any(|step| matches!(
                    step.node,
                    Node::Control {
                        control: Control::Match { .. }
                    }
                )),
            "the branches lower to one recipient match"
        );
    }
}

#[test]
fn by_kind_needs_exactly_one_chosen_recipient_and_keeps_its_names_local() {
    // Controls: each shape lowers without the by_kind step.
    spell(
        &json!({"bind":"t","from":{"who":"self","zone":"field","kind":"follower"},"n":1}),
        &json!([{"buff":{"on":"t","hp":2}}]),
    )
    .unwrap();
    spell(&leader_or("upto"), &json!([{"damage":{"to":"t","n":1}}])).unwrap();
    // An ordinary object target has no leader branch to narrow.
    spell(
        &json!({"bind":"t","from":{"who":"self","zone":"field","kind":"follower"},"n":1}),
        &json!([by_kind(None)]),
    )
    .unwrap_err();
    // An optional choice may hold no recipient, so it has no single kind.
    spell(&leader_or("upto"), &json!([by_kind(None)])).unwrap_err();
    // The narrowed name ends with its branch.
    spell(
        &leader_or("n"),
        &json!([by_kind(None), {"life":{"who":"item","delta":1}}]),
    )
    .unwrap_err();
    // A leader branch cannot treat the leader as a follower.
    spell(
        &leader_or("n"),
        &json!([{"by_kind":{"ref":"t","leader":[{"buff":{"on":"item","hp":2}}],"follower":[]}}]),
    )
    .unwrap_err();
}

/// Followers named after themselves, so a name filter can tell them apart.
fn named_catalog() -> Arc<Catalog> {
    let mut lines = vec![];
    let mut cards = serde_json::Map::new();
    for (number, kind) in [
        ("unit-spell", "スペル"),
        ("unit-follower", "フォロワー"),
        ("マホ", "フォロワー"),
        ("other", "フォロワー"),
    ] {
        lines.push(json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]}]}).to_string());
        cards.insert(
            number.into(),
            json!({"status":"complete","review":"synthetic","abilities":[]}),
        );
    }
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1,"kind":"effect_set","cards":cards}).to_string();
    Arc::new(
        Catalog::from_documents(
            &lines.join("\n"),
            &registry,
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    )
}

/// P1 plays the spell with the given field, answering the target with the card `pick`
/// (None is the leader); returns the offered cards and the final referee view.
fn play(
    program: &WellFormedCardProgram,
    field: &Value,
    pick: Option<&str>,
) -> (Vec<Option<String>>, Value) {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = field.clone();
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        named_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "by-kind",
    )
    .unwrap();
    game.load_internal_card("unit-spell", program).unwrap();
    for number in ["unit-follower", "マホ", "other"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let mut offered = Vec::new();
    let mut step = Some(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    while let Some(CardPlay::Awaiting(q)) = step.take() {
        let InputDomain::Targets(domain) = &q.domain else {
            break;
        };
        offered = domain.members.iter().map(|m| m.card.clone()).collect();
        let chosen = domain
            .members
            .iter()
            .filter(|m| m.card.as_deref() == pick)
            .map(|m| m.handle)
            .take(1)
            .collect();
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Targets(chosen))
                .unwrap(),
        );
    }
    (offered, game.projection(View::Referee).unwrap())
}

fn hp(view: &Value, id: &str) -> Value {
    view["objects"][id]["hp"].clone()
}

#[test]
fn a_leader_or_named_follower_union_keeps_the_name() {
    for name in [json!({"name":"マホ"}), json!({"name_contains":"ホ"})] {
        let mut follower = json!({"who":"self","zone":"field","kind":"follower"});
        follower
            .as_object_mut()
            .unwrap()
            .extend(name.as_object().unwrap().clone());
        let program = spell(
            &json!({"bind":"t","from":{"any_of":[{"leader":"self"},follower]},"n":1}),
            &json!([by_kind(None)]),
        )
        .unwrap();
        let field = json!([{"id":"maho","card":"マホ"},{"id":"plain","card":"other"}]);
        let (offered, view) = play(&program, &field, Some("マホ"));
        assert_eq!(
            offered,
            [None, Some("マホ".to_owned())],
            "the differently named follower is not a candidate"
        );
        assert_eq!(hp(&view, "maho"), 3_u8);
        assert_eq!(hp(&view, "plain"), 1_u8);
    }
}

#[test]
fn by_kind_inside_a_fixed_repeat_is_admitted_and_runs() {
    let program = spell(
        &leader_or("n"),
        &json!([{"repeat":{"n":1,"do":[by_kind(None)]}}]),
    )
    .unwrap();
    let (_, view) = play(
        &program,
        &json!([{"id":"ally","card":"unit-follower"}]),
        None,
    );
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
    assert_eq!(hp(&view, "ally"), 1_u8);
}

/// A field follower whose end-phase auto pays PP1, then runs `by_kind` with `follower` steps.
fn paying_follower(follower: &Value) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let card = &tree["cards"]["BP08-038"];
    let meta = card["meta"].clone();
    let mut entry = card["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    let by_kind = json!({"by_kind":{"ref":"t","leader":[{"life":{"who":"item","delta":2}}],
        "follower":follower}});
    entry["auto"] = json!({
        "on":{"phase":{"whose":"self","which":"end"}},
        "targets":[leader_or("n")],
        "do":[{"pay":{"cost":{"pp":1},"do":[by_kind]}}]
    });
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn paying_game() -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"src","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    Game::new(
        named_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "by-kind-pay",
    )
    .unwrap()
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Any other question means the payment flow under test went wrong."
)]
fn by_kind_inside_a_triggered_payment_runs_after_paying() {
    let program =
        paying_follower(&json!([{"buff":{"on":"item","power":1}},{"buff":{"on":"item","hp":2}}]));
    let mut game = paying_game();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let mut step = None;
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(domain) => Answer::Targets(
                    domain
                        .members
                        .iter()
                        .filter(|member| member.card.is_none())
                        .map(|member| member.handle)
                        .collect(),
                ),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                other => panic!("unexpected question: {other:?}"),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            break;
        };
        step = Some(CardPlay::Awaiting(q));
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 1_u8, "PP1 was paid");
    assert_eq!(
        view["P1"]["leader"]["life"], 12_u8,
        "the paid leader branch ran"
    );
}

#[test]
fn a_paid_kind_branch_shares_the_execution_timestamp_checks() {
    for follower in [
        json!([{"set_stat":{"on":"item","power":1}},{"buff":{"on":"item","power":2}}]),
        json!([{"set_stat":{"on":"item","hp":1}},{"set_stat":{"on":"item","hp":3}}]),
    ] {
        paying_game()
            .load_internal_card("unit-follower", &paying_follower(&follower))
            .unwrap_err();
    }
}

#[test]
fn kind_branches_share_the_execution_timestamp_checks() {
    let load = |steps: &Value| {
        let program = spell(&leader_or("n"), steps).unwrap();
        let mut game = Game::new(
            named_catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "ts",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &program)
    };
    let branch = |follower: Value| json!({"by_kind":{"ref":"t","leader":[{"life":{"who":"item","delta":1}}],"follower":follower}});
    load(&json!([branch(
        json!([{"buff":{"on":"item","power":2}},{"buff":{"on":"item","hp":2}}])
    )]))
    .unwrap();
    load(&json!([branch(
        json!([{"set_stat":{"on":"item","power":1}},{"buff":{"on":"item","power":2}}])
    )]))
    .unwrap_err();
    load(&json!([branch(
        json!([{"set_stat":{"on":"item","hp":1}},{"set_stat":{"on":"item","hp":3}}])
    )]))
    .unwrap_err();
    // A branch write still conflicts with one outside the branch.
    load(&json!([
        {"set_stat":{"on":{"who":"self","zone":"field","kind":"follower"},"power":1}},
        branch(json!([{"buff":{"on":"item","power":2}}]))
    ]))
    .unwrap_err();
}
