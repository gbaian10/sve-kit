use super::*;
use crate::rule_fixtures;
use std::sync::OnceLock;
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::InputDomain;

fn header() -> Value {
    json!({"ub_activated":{"who":"self","holder":{"who":"self","zone":"field","kind":"follower","other":true}}})
}
fn tree(ub: bool, depart: bool, cost: u8) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP01-041"]["abilities"][0].clone();
    entry["line"] = json!(1_u8);
    entry["activate"] = json!({"cost":{"pp":cost},"do":if depart {
        json!([{"move":{"ref":{"who":"self","zone":"field","kind":"follower","other":true},"to":"cemetery"}}])
    } else { json!([]) }});
    if ub {
        entry["activate"]["ub"] = json!(true);
    }
    let mut observer = entry.clone();
    observer["line"] = json!(2_u8);
    observer.as_object_mut().unwrap().remove("activate");
    observer["auto"] = json!({"on":header(),"do":[{"life":{"who":"self","delta":1}}]});
    let meta = tree["cards"]["BP01-041"]["meta"].clone();
    let card = json!({"meta":meta,"abilities":[entry,observer]});
    tree["cards"] = json!({"unit-follower":card,"unit-amulet":card});
    tree
}
fn build(ub: bool, depart: bool, cost: u8, source_kind: &str, seat: u32) -> (Game, AbilityId) {
    build_from(&tree(ub, depart, cost), source_kind, seat)
}
pub(super) fn title_catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let (snapshot, keywords, document) = catalog_documents(&json!("1"));
        let mut rows = snapshot
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        let mut follower = rows[1].clone();
        follower["number"] = json!("unit-other-follower");
        rows.push(follower);
        let mut equipment = rows[1].clone();
        equipment["number"] = json!("unit-equipment");
        equipment["faces"][0]["name"] = json!("unit-equipment");
        equipment["faces"][0]["card_type"] = json!("イクイップメント・トークン");
        equipment["faces"][0]["power"] = json!("-");
        equipment["faces"][0]["hp"] = json!("-");
        rows.push(equipment);
        let snapshot = rows
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let mut document: Value = serde_json::from_str(&document).unwrap();
        document["cards"]["unit-other-follower"] = document["cards"]["unit-follower"].clone();
        document["cards"]["unit-equipment"] = document["cards"]["unit-follower"].clone();
        let document = document.to_string();
        let snapshot = snapshot
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                row["faces"][0]["title"] = json!("UB test");
                row.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let rules = rule_fixtures::rules(
            &snapshot,
            json!([rule_fixtures::title(
                "ub_test",
                "unit-follower",
                json!([{"kind":"ub_enabled"}])
            )]),
            json!([]),
        );
        Arc::new(
            Catalog::from_documents_with_rules(
                &snapshot,
                &keywords,
                &[("unit.yaml".into(), document)],
                &rules.to_string(),
                &EngineIdentityInput::LegacyJp,
            )
            .unwrap(),
        )
    }))
}

fn build_from(input: &Value, source_kind: &str, seat: u32) -> (Game, AbilityId) {
    let programs = lowerer().lower(input).unwrap();
    let catalog = title_catalog();
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(if seat == 1 { "P1" } else { "P2" });
    for side in ["P1", "P2"] {
        initial["players"][side]["construction"] = json!("title");
        initial["players"][side]["title_code"] = json!("ub_test");
        initial["players"][side]["zones"]["hand"] = json!([]);
        initial["players"][side]["zones"]["field"] = json!([]);
    }
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-follower"}]);
    let actor = if seat == 1 { "P1" } else { "P2" };
    initial["players"][actor]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":source_kind}));
    let mut game = Game::new(catalog, &initial, &Value::Null, &Value::Null, "ub-event").unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let definition = programs[source_kind]
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    (game, definition)
}
pub(super) fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
#[test]
fn ub_headers_keep_the_unadmitted_variants_closed() {
    for (pointer, value) in [
        ("/who", json!("opp")),
        ("/holder/who", json!("opp")),
        ("/holder/zone", json!("cemetery")),
        ("/holder/kind", json!("amulet")),
        ("/holder/other", json!(false)),
    ] {
        let mut input = tree(true, false, 0);
        *input["cards"]["unit-follower"]["abilities"][1]["auto"]["on"]["ub_activated"]
            .pointer_mut(pointer)
            .unwrap() = value;
        assert!(
            lowerer().lower(&input).is_err(),
            "only other own field followers are admitted"
        );
    }
}
#[test]
fn only_a_committed_ub_from_another_own_field_follower_produces_one_occurrence() {
    for (ub, kind, seat, expected) in [
        (true, "unit-follower", 1, true),
        (false, "unit-follower", 1, false),
        (true, "unit-follower", 2, false),
        (true, "unit-amulet", 1, false),
    ] {
        let (mut game, definition) = build(ub, false, 0, kind, seat);
        resolved(
            game.activate_internal_card("producer", definition, PlayerId { seat })
                .unwrap(),
        );
        finish(&mut game);
        let pending = game.internal_pending_choice().unwrap();
        assert_eq!(pending.is_some(), expected);
        if let Some(question) = pending {
            assert_eq!(
                game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]
                    ["ub_activated"],
                1_u8,
                "the formal UB is already counted before its observers resolve"
            );
            let InputDomain::Pending(options) = &question.domain else {
                panic!("UB pending");
            };
            assert_eq!(options.len(), 1, "self and opposing observers do not match");
            assert!(options[0].event.starts_with("UbActivated"));
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            finish(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                21_u8
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}
#[test]
fn observer_departure_in_the_ub_body_does_not_erase_the_committed_occurrence() {
    let (mut game, definition) = build(true, true, 0, "unit-follower", 1);
    resolved(
        game.activate_internal_card("producer", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    finish(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("observer"))
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("retained UB occurrence");
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
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21_u8
    );
}
#[test]
fn unaffordable_ub_does_not_emit_an_occurrence() {
    let (mut game, definition) = build(true, false, 3, "unit-follower", 1);
    game.activate_internal_card("producer", definition, PlayerId { seat: 1 })
        .unwrap_err();
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        20_u8
    );
}

#[test]
fn the_pending_body_binds_it_to_the_committed_generation_of_the_ub_holder() {
    for depart in [false, true] {
        let mut input = tree(true, false, 0);
        input["cards"]["unit-follower"]["abilities"][1]["auto"]["do"] =
            json!([{"buff":{"on":"it","power":1,"hp":1}}]);
        if depart {
            input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] =
                json!([{"move":{"ref":"this","to":"cemetery"}}]);
        }
        let (mut game, definition) = build_from(&input, "unit-follower", 1);
        resolved(
            game.activate_internal_card("producer", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        finish(&mut game);
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("UB holder binding");
        };
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"][if depart { "cemetery" } else { "field" }]
                .as_array()
                .unwrap()
                .contains(&json!("producer"))
        );
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::StatChange(_))),
            !depart,
            "an event binding cannot buff a later generation in another zone"
        );
        if !depart {
            assert_eq!(state["objects"]["producer"]["power"], 3_u8);
            assert_eq!(state["objects"]["producer"]["hp"], 4_u8);
        }
        assert_eq!(state["objects"]["observer"]["power"], 2_u8);
    }
}

#[test]
fn an_observer_paid_to_the_cemetery_before_the_ub_commit_does_not_trigger() {
    let mut input = tree(true, false, 0);
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"] = json!({"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"follower","other":true},"n":1}});
    let (mut game, definition) = build_from(&input, "unit-follower", 1);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("producer", definition, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("one material cost");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("public field material");
    };
    assert_eq!(domain.members.len(), 1);
    let activation = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert!(activation.progress.facts.iter().any(|fact| matches!(
        fact,
        Fact::AbilityPlayed {
            qualification: AbilityQualification::UnionBurst,
            ..
        }
    )));
    finish(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("observer"))
    );
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert!(game.internal_pending_choice().unwrap().is_none());
}
