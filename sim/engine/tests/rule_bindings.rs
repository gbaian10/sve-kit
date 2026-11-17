//! Synthetic identities, finite capabilities and input-boundary rejection cases.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Small synthetic fixtures fail immediately on a malformed construction or missing result."
)]

extern crate alloc;
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use serde_json::{Value, json};
use std::env::temp_dir;
use std::fs::{create_dir_all, remove_dir_all, write};
use std::process::id;
use sve_engine::EngineFailure;
use sve_engine::catalog::{Catalog, EngineIdentityInput, FaceIdentity};
use sve_engine::game::{Game, View};

#[path = "support/rule_fixtures.rs"]
mod rule_fixtures;

#[derive(Clone)]
struct Fixture {
    facts: Vec<Value>,
    programs: Value,
    keywords: Value,
    rules: Value,
}

impl Fixture {
    fn new() -> Self {
        let definitions = [
            ("actor", "synthetic-actor", "フォロワー", "start-label"),
            (
                "lesson",
                "synthetic-printed-resource",
                "スペル・トークン",
                "lesson-label",
            ),
            ("meal", "synthetic-meal", "エボルヴ・スペル", "ub-label"),
            (
                "drive",
                "synthetic-drive",
                "エボルヴ・スペル",
                "empty-label",
            ),
            ("stack", "synthetic-stack", "アミュレット・トークン", ""),
            ("alias", "synthetic-alias", "スペル", ""),
        ];
        let facts = definitions.iter().map(|(number, name, kind, title)| json!({"number":number,"faces":[{"name":name,"card_type":kind,"title":title,"card_class":"ニュートラル","cost":"0","power":"2","hp":"3","traits":[],"sections":[]}]})).collect();
        let mut programs = json!({"format":1_u8,"kind":"effect_set","cards":{}});
        for (number, ..) in definitions {
            programs["cards"][number] = json!({"status":"complete","abilities":[]});
        }
        programs["cards"]["lesson"]["rules_name"] = json!("synthetic-lesson");
        programs["cards"]["alias"]["abilities"] = json!([{"kind":"static","line":1_u8,"body":{"op":"name_alias","subjects":"self","name":"synthetic-lesson","while_zone":"any"}}]);
        let mut fixture = Self {
            facts,
            programs,
            keywords: json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}),
            rules: Value::Null,
        };
        fixture.rules = rule_fixtures::rules(
            &fixture.snapshot(),
            json!([
                rule_fixtures::title(
                    "synthetic_start",
                    "actor",
                    json!([{"kind":"opening_start_amulet"}])
                ),
                rule_fixtures::title(
                    "synthetic_lesson",
                    "lesson",
                    json!([{"kind":"opening_ex_resource","resource_role":"lesson_item","count":5_u8,"zone":"ex"}])
                ),
                rule_fixtures::title("synthetic_ub", "meal", json!([{"kind":"ub_enabled"}])),
                rule_fixtures::title("synthetic_empty", "drive", json!([]))
            ]),
            json!([
                rule_fixtures::resource("lesson_item", "lesson", "rules", true),
                rule_fixtures::resource("meal_item", "meal", "printed", false),
                rule_fixtures::resource("drive_point", "drive", "printed", false),
                rule_fixtures::resource("stack_base", "stack", "printed", true)
            ]),
        );
        fixture
    }

    fn snapshot(&self) -> String {
        self.facts
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn repin(&mut self) {
        self.rules["input"]["snapshot_sha256"] =
            json!(rule_fixtures::hash(self.snapshot().as_bytes()));
    }

    fn documents(&self) -> Vec<(String, String)> {
        vec![("synthetic.yaml".into(), self.programs.to_string())]
    }

    fn load(&self) -> Result<Catalog, EngineFailure> {
        Catalog::from_documents_with_rules(
            &self.snapshot(),
            &self.keywords.to_string(),
            &self.documents(),
            &self.rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
    }

    fn game(&self, position: &Value) -> Result<Game, EngineFailure> {
        from_catalog(Arc::new(self.load()?), position)
    }
}

fn from_catalog(catalog: Arc<Catalog>, position: &Value) -> Result<Game, EngineFailure> {
    Game::new(catalog, position, &Value::Null, &Value::Null, "synthetic")
}

fn position() -> Value {
    json!({"history":"none","turn":{"active":"P1","phase":"main","first_player":"P1","elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{
        "P1":{"construction":"class","leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":3_u8,"max":3_u8},"ep":1_u8,"sep":0_u8,"zones":{"field":[{"id":"actor","card":"actor"}],"deck":[{"filler":10_u8}]}},
        "P2":{"construction":"class","leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":3_u8,"max":3_u8},"ep":1_u8,"sep":0_u8,"zones":{"deck":[{"filler":10_u8}]}}}})
}

#[test]
fn registered_empty_title_resolves_and_unknown_or_conflicting_title_fails() {
    let fixture = Fixture::new();
    let catalog = Arc::new(fixture.load().unwrap());
    let mut setup = position();
    setup["players"]["P1"]["construction"] = json!("title");
    setup["players"]["P1"]["title_code"] = json!("synthetic_empty");
    let game = from_catalog(Arc::clone(&catalog), &setup).unwrap();
    assert_eq!(
        game.projection(View::P1).unwrap()["P1"]["title_code"],
        "synthetic_empty"
    );
    for code in ["unknown_title", ""] {
        setup["players"]["P1"]["title_code"] = json!(code);
        assert!(matches!(
            from_catalog(Arc::clone(&catalog), &setup),
            Err(EngineFailure::Unsupported(_))
        ));
    }
    setup["players"]["P1"]["title_code"] = json!("synthetic_ub");
    setup["players"]["P1"]["title"] = json!("empty-label");
    assert!(matches!(
        from_catalog(Arc::clone(&catalog), &setup),
        Err(EngineFailure::Invalid(_))
    ));
    setup["players"]["P1"]["title_code"] = Value::Null;
    from_catalog(Arc::clone(&catalog), &setup).unwrap();
}

#[test]
fn malformed_settings_business_keys_references_templates_and_background_fail() {
    let original = Fixture::new();
    let mutations: Vec<fn(&mut Fixture)> = vec![
        |f| {
            f.rules["unexpected"] = json!(true);
        },
        |f| {
            f.rules["titles"][1]["title_code"] = json!("synthetic_start");
        },
        |f| {
            f.rules["resources"][1]["role"] = json!("drive_point");
        },
        |f| {
            f.rules["titles"][0]["anchor"]["region"] = json!("en");
        },
        |f| {
            f.rules["resources"][0]["names"][0]["face"]["region"] = json!("en");
        },
        |f| {
            f.rules["resources"][0]["template"]["region"] = json!("en");
        },
        |f| {
            f.rules["titles"][0]["anchor"]["card_no"] = json!("absent");
        },
        |f| {
            f.rules["resources"][0]["template"]["face_ordinal"] = json!(4_u8);
        },
        |f| {
            f.rules["resources"][0]["template"]["card_no"] = json!("actor");
        },
        |f| {
            f.rules["resources"][0]["template"]["card_no"] = json!("stack");
        },
        |f| {
            f.programs["cards"]["lesson"]["status"] = json!("partial");
        },
        |f| {
            f.programs["cards"]["lesson"]["abilities"] = json!([{"kind":"spell","line":1_u8,"body":{"op":"draw","count":{"read":"self.unimplemented"}}}]);
        },
        |f| {
            f.rules["input"]["snapshot_sha256"] = json!("b".repeat(64));
        },
        |f| {
            f.rules["input"]["kind"] = json!("resolved");
        },
        |f| {
            f.rules["scope"]["region"] = json!("en");
        },
        |f| {
            f.rules["titles"][0]["evidence"] = json!(["missing"]);
        },
        |f| {
            let mut extra = f.rules["evidence"][0].clone();
            extra["id"] = json!("unused");
            f.rules["evidence"].as_array_mut().unwrap().push(extra);
        },
        |f| {
            let mut extra = f.rules["evidence"][0].clone();
            extra["rule_refs"] = json!(["2.3.4"]);
            f.rules["evidence"].as_array_mut().unwrap().push(extra);
        },
        |f| {
            f.rules["evidence"][0]["checked_on"] = json!("2026-02-30");
        },
        |f| {
            f.rules["titles"][0]["capabilities"] = json!([{"kind":"unregistered"}]);
        },
        |f| {
            f.rules["titles"][1]["capabilities"][0]["count"] = json!(6_u8);
        },
        |f| {
            f.facts[0]["faces"][0]["title"] = json!("empty-label");
            f.repin();
        },
        |f| {
            f.rules["resources"][1]["names"] = f.rules["resources"][2]["names"].clone();
        },
        |f| {
            f.rules["resources"].as_array_mut().unwrap().remove(0);
        },
    ];
    for (ordinal, mutate) in mutations.into_iter().enumerate() {
        let mut fixture = original.clone();
        mutate(&mut fixture);
        assert!(fixture.load().is_err(), "mutation {ordinal} was accepted");
    }
}

#[test]
fn document_envelopes_are_exclusive_and_background_names_the_grammar() {
    let original = Fixture::new();
    original.load().unwrap();
    for area in ["effects", "keywords", "engine"] {
        for (field, value) in [
            ("format", json!(true)),
            ("format", json!("1")),
            ("format", json!(1.0_f64)),
            ("format", json!(999_u16)),
            ("kind", json!("unknown")),
            ("version", json!("astra/1")),
        ] {
            let mut fixture = original.clone();
            let document = match area {
                "effects" => &mut fixture.programs,
                "keywords" => &mut fixture.keywords,
                _ => &mut fixture.rules,
            };
            document[field] = value;
            assert!(fixture.load().is_err(), "{area}/{field} was accepted");
        }
        for field in ["format", "kind"] {
            let mut fixture = original.clone();
            let document = match area {
                "effects" => &mut fixture.programs,
                "keywords" => &mut fixture.keywords,
                _ => &mut fixture.rules,
            };
            document.as_object_mut().unwrap().remove(field);
            assert!(fixture.load().is_err(), "{area}/{field} could be omitted");
        }
    }
    let game = original.game(&position()).unwrap();
    let saved = serde_json::to_value(&game).unwrap();
    let versions = &saved["catalog"]["rule_bindings"]["background"]["Bound"]["schema_versions"];
    assert_eq!(
        versions["effects"],
        json!({"kind":"effect_set","format":1_u8,"schema_id":"urn:sve-kit:effects:astra:1"})
    );
    assert_eq!(
        versions["keywords"],
        json!({"kind":"keyword_registry","format":1_u8,"schema_id":"urn:sve-kit:effects:astra:1"})
    );
    assert_eq!(
        versions["engine_rules"],
        json!({"kind":"engine_rules","format":1_u8,"schema_id":"https://svekit.app/dsl/engine-rules.schema.json"})
    );
}

fn project(fixture: &Fixture) -> (Value, EngineIdentityInput) {
    let mut rules = fixture.rules.clone();
    rules["input"]["kind"] = json!("resolved");
    let faces = fixture
        .facts
        .iter()
        .map(|card| {
            let number = card["number"].as_str().unwrap();
            let title_code = fixture.rules["titles"]
                .as_array()
                .unwrap()
                .iter()
                .find(|title| title["anchor"]["card_no"] == number)
                .map(|title| title["title_code"].as_str().unwrap().to_owned());
            FaceIdentity {
                card_no: number.into(),
                face_ordinal: 0,
                face_id: format!("face:{number}"),
                rules_name_id: format!("rules:{number}"),
                title_code,
            }
        })
        .collect::<Vec<_>>();
    for title in rules["titles"].as_array_mut().unwrap() {
        let number = title["anchor"]["card_no"].as_str().unwrap();
        title["anchor"] =
            json!({"kind":"face-id","region":"jp","face_id":format!("face:{number}")});
    }
    for resource in rules["resources"].as_array_mut().unwrap() {
        let number = resource["names"][0]["face"]["card_no"]
            .as_str()
            .unwrap()
            .to_owned();
        resource["names"] = json!([{"kind":"rules-name-id","region":"jp","rules_name_id":format!("rules:{number}")}]);
        if resource.get("template").is_some() {
            resource["template"] =
                json!({"kind":"face-id","region":"jp","face_id":format!("face:{number}")});
        }
    }
    let identity = EngineIdentityInput::Resolved {
        region: "jp".into(),
        snapshot_sha256: rule_fixtures::hash(fixture.snapshot().as_bytes()),
        faces,
    };
    (rules, identity)
}

#[test]
fn resolved_projection_is_explicit_and_rejects_unknown_or_inconsistent_identities() {
    let fixture = Fixture::new();
    let (rules, identity) = project(&fixture);
    let load = |settings: &Value, input: &EngineIdentityInput| {
        Catalog::from_documents_with_rules(
            &fixture.snapshot(),
            &fixture.keywords.to_string(),
            &fixture.documents(),
            &settings.to_string(),
            input,
        )
    };
    let loaded = load(&rules, &identity).unwrap();
    let mut setup = position();
    setup["players"]["P1"]["construction"] = json!("title");
    setup["players"]["P1"]["title_code"] = json!("synthetic_empty");
    Game::new(
        Arc::new(loaded),
        &setup,
        &Value::Null,
        &Value::Null,
        "projected",
    )
    .unwrap();
    for mutation in 0_u8..6 {
        let mut changed = identity.clone();
        if let EngineIdentityInput::Resolved {
            region,
            snapshot_sha256,
            faces,
        } = &mut changed
        {
            match mutation {
                0 => *region = "en".into(),
                1 => *snapshot_sha256 = "c".repeat(64),
                2 => faces[0].card_no = "absent".into(),
                3 => faces[0].title_code = Some("synthetic_empty".into()),
                4 => faces[0].face_id = faces[1].face_id.clone(),
                _ => faces[0].rules_name_id = faces[1].rules_name_id.clone(),
            }
        }
        assert!(
            load(&rules, &changed).is_err(),
            "projection mutation {mutation}"
        );
    }
    let mut changed = rules.clone();
    changed["resources"][0]["names"][0]["rules_name_id"] = json!("absent");
    load(&changed, &identity).unwrap_err();
    changed = rules;
    changed["titles"][0]["anchor"]["face_id"] = json!("absent");
    load(&changed, &identity).unwrap_err();
}

#[test]
fn file_and_memory_loaders_have_identical_binding_backgrounds() {
    let fixture = Fixture::new();
    let root = temp_dir().join(format!("sve-engine-bindings-{}", id()));
    create_dir_all(root.join("rules/effects")).unwrap();
    create_dir_all(root.join("rules/engine")).unwrap();
    write(root.join("cards.jsonl"), fixture.snapshot()).unwrap();
    write(
        root.join("rules/keywords.yaml"),
        fixture.keywords.to_string(),
    )
    .unwrap();
    write(
        root.join("rules/effects/unit.yaml"),
        fixture.programs.to_string(),
    )
    .unwrap();
    write(
        root.join("rules/engine/index.yaml"),
        fixture.rules.to_string(),
    )
    .unwrap();
    let file = Catalog::load(&root.join("cards.jsonl"), &root).unwrap();
    let memory = fixture.load().unwrap();
    remove_dir_all(root).unwrap();
    assert_eq!(
        serde_json::to_value(file).unwrap(),
        serde_json::to_value(memory).unwrap()
    );
}

fn resource_programs(fixture: &mut Fixture) {
    fixture.programs["cards"]["actor"]["abilities"] = json!([
        {"line":1_u8,"kind":"activated","costs":[{"op":"lesson","count":1_u8}],"body":{"op":"draw","count":0_u8}},
        {"line":2_u8,"kind":"activated","costs":[{"op":"eat","subjects":"self","count":1_u8}],"body":{"op":"draw","count":0_u8}},
        {"line":3_u8,"kind":"ride","costs":[{"op":"pp","amount":0_u8}],"body":{"op":"gain_drive","subjects":"self"}},
        {"line":4_u8,"kind":"activated","ub":true,"body":{"op":"pp","amount":1_u8}},
        {"line":5_u8,"kind":"activated","targets":[{"key":"1","select":{"side":"both","zone":"any"},"min":1_u8,"max":1_u8}],"body":{"op":"banish","subjects":"target.1"}},
        {"line":6_u8,"kind":"activated","body":{"op":"stack","amount":1_u8}},
        {"line":7_u8,"kind":"activated","targets":[{"key":"1","select":{"side":"both","zone":"any"},"min":1_u8,"max":1_u8}],"body":{"op":"move","subjects":"target.1","to":"cemetery"}}
    ]);
    fixture.keywords["keywords"]["stack"] =
        json!({"ja":"synthetic-stack-keyword","expansion":{"op":"keyword","name":"stack"}});
    fixture.keywords["keywords"]["stack_counter"] = json!({"ja":"synthetic-stack-counter","role":"counter","expansion":{"op":"keyword","name":"stack_counter"}});
    fixture.programs["cards"]["stack"]["abilities"] =
        json!([{"kind":"static","line":1_u8,"body":{"op":"keyword","name":"stack"}}]);
}

fn choices(game: &Game, line: u64) -> Vec<Value> {
    game.legal()
        .unwrap()
        .into_iter()
        .filter(|action| action["do"] == "activate" && action["ability"]["line"] == line)
        .collect()
}

#[test]
fn cost_roles_match_current_aliases_and_keep_zone_controller_and_face_up_filters() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    fixture.programs["cards"]["actor"]["abilities"][0]["body"] = json!({"op":"modify","subjects":"self.leader","hp":{"read":"self.turn.magic_item_banished"}});
    let catalog = Arc::new(fixture.load().unwrap());
    let mut setup = position();
    setup["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"lesson","card":"lesson"},{"id":"alias","card":"alias"},{"id":"wrong","card":"meal"}]);
    setup["players"]["P2"]["zones"]["ex"] = json!([{"id":"opponent-alias","card":"alias"}]);
    setup["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"meal","card":"meal"},{"id":"drive","card":"drive"},{"id":"up","card":"drive","face_up":true}]);
    let game = from_catalog(Arc::clone(&catalog), &setup).unwrap();
    let material_ids = |line| {
        choices(&game, line)
            .iter()
            .map(|action| action["costs"]["1"][0].as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(material_ids(1), ["lesson".into(), "alias".into()].into());
    assert_eq!(material_ids(2), ["meal".into()].into());
    assert_eq!(material_ids(3), ["drive".into()].into());
    for material in ["lesson", "alias"] {
        let mut instance = game.clone();
        let choice = choices(&instance, 1)
            .into_iter()
            .find(|action| action["costs"]["1"][0] == material)
            .unwrap();
        assert_eq!(
            instance.decide(&choice, "resource").unwrap().outcome,
            "resolved"
        );
        let actual = instance.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]["magic_item_banished"].as_i64().unwrap_or(0);
        assert_eq!(actual, i64::from(material == "lesson"));
        assert_eq!(
            instance.query(View::Referee, "P1.leader.life").unwrap(),
            Some(json!(20_i64 + actual))
        );
        if material == "lesson" {
            assert!(
                instance.projection(View::Referee).unwrap()["objects"]
                    .get("lesson")
                    .is_none()
            );
        }
    }
    for line in [2_u64, 3] {
        let mut paid = game.clone();
        let action = choices(&paid, line).remove(0);
        assert_eq!(
            paid.decide(&action, "pay-role").unwrap().outcome,
            "resolved"
        );
        let packet = paid.projection(View::Referee).unwrap();
        if line == 2 {
            assert_eq!(packet["P1"]["race"], json!(["meal"]));
            assert_eq!(packet["objects"]["actor"]["links"]["出走"], json!(["meal"]));
        } else {
            assert_eq!(packet["P1"]["drive"], json!(["drive"]));
            assert!(
                !packet["P1"]["evolve_deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("drive"))
            );
        }
    }
    setup["players"]["P1"]["zones"]["ex"] = json!([]);
    setup["players"]["P1"]["zones"]["hand"] = json!([{"id":"alias","card":"alias"}]);
    assert_eq!(
        choices(&from_catalog(Arc::clone(&catalog), &setup).unwrap(), 1),
        Vec::<Value>::new()
    );
}

#[test]
fn role_dependencies_and_opening_templates_fail_at_load_without_partial_state() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    for role in ["meal_item", "drive_point", "stack_base"] {
        let mut changed = fixture.clone();
        changed.rules["resources"]
            .as_array_mut()
            .unwrap()
            .retain(|entry| entry["role"] != role);
        assert!(changed.load().is_err(), "{role}");
    }
    for role in ["lesson_item", "stack_base"] {
        let mut changed = fixture.clone();
        changed.rules["resources"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["role"] == role)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("template");
        assert!(changed.load().is_err(), "{role}");
    }
    let unbound = Catalog::from_documents(
        &fixture.snapshot(),
        &fixture.keywords.to_string(),
        &fixture.documents(),
    )
    .unwrap();
    let mut setup = position();
    setup["players"]["P1"]["zones"]["ex"] = json!([{"id":"target","card":"lesson"}]);
    let mut game = Game::new(
        Arc::new(unbound),
        &setup,
        &Value::Null,
        &Value::Null,
        "unbound",
    )
    .unwrap();
    let before = game.digest().unwrap();
    let error = game
        .decide(
            &json!({"do":"activate","ability":{"source":"actor","line":1_u8}}),
            "unbound",
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unbound resource role: lesson_item"),
        "{error}"
    );
    assert_eq!(before, game.digest().unwrap());
}

#[test]
fn legacy_and_role_ride_forms_pay_once_and_malformed_forms_stay_unsupported() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    let implicit = fixture.programs["cards"]["actor"]["abilities"][2].clone();
    let valid = json!({"op":"link_resource","subjects":"self","resource_role":"drive_point","count":1_u8,"from_zone":"evolve_deck","to":"drive"});
    let mut legacy = valid.clone();
    legacy.as_object_mut().unwrap().remove("resource_role");
    legacy["name"] = json!("synthetic-drive");
    let mut setup = position();
    setup["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"drive","card":"drive"}]);
    let mut expected = None;
    for link in [None, Some(valid.clone()), Some(legacy)] {
        fixture.programs["cards"]["actor"]["abilities"][2] = implicit.clone();
        if let Some(link) = link {
            fixture.programs["cards"]["actor"]["abilities"][2]["costs"]
                .as_array_mut()
                .unwrap()
                .push(link);
        }
        let mut game = fixture.game(&setup).unwrap();
        let actions = choices(&game, 3);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["costs"], json!({"1":["drive"]}));
        assert_eq!(
            game.decide(&actions[0], "ride").unwrap().outcome,
            "resolved"
        );
        let result = game.query(View::Referee, "P1.drive").unwrap();
        if let Some(previous) = &expected {
            assert_eq!(&result, previous);
        }
        expected = Some(result);
    }
    for (key, wrong) in [
        ("subjects", json!("self.leader")),
        ("count", json!(2_u8)),
        ("from_zone", json!("ex")),
        ("to", json!("cemetery")),
        ("resource_role", json!("meal_item")),
    ] {
        let mut link = valid.clone();
        link[key] = wrong;
        fixture.programs["cards"]["actor"]["abilities"][2] = implicit.clone();
        fixture.programs["cards"]["actor"]["abilities"][2]["costs"]
            .as_array_mut()
            .unwrap()
            .push(link);
        let game = fixture.game(&setup).unwrap();
        assert!(
            matches!(game.legal(), Err(EngineFailure::Unsupported(_))),
            "{key}"
        );
    }
}

#[test]
fn same_version_save_restores_pending_stack_and_hides_private_ids() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    let stack = fixture.programs["cards"]["actor"]["abilities"][5].clone();
    fixture.programs["cards"]["actor"]["abilities"] = json!([stack]);
    let mut setup = position();
    setup["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"stack","card":"stack","state":{"counters":{"stack_counter":1_u8}}}));
    setup["players"]["P2"]["zones"]["hand"] = json!([{"id":"private-opponent-id","card":"alias"}]);
    let mut original = fixture.game(&setup).unwrap();
    let step = original
        .decide(
            &json!({"do":"activate","ability":{"source":"actor","line":6_u8}}),
            "bindings",
        )
        .unwrap();
    assert_eq!(step.outcome, "paused");
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    assert_eq!(original.legal().unwrap(), restored.legal().unwrap());
    assert!(
        !restored
            .projection(View::P1)
            .unwrap()
            .to_string()
            .contains("private-opponent-id")
    );
    let action = restored.legal().unwrap().remove(0);
    let original_step = original.decide(&action, "resolve").unwrap();
    let restored_step = restored.decide(&action, "resolve").unwrap();
    assert_eq!(original_step.events, restored_step.events);
    assert_eq!(original.digest().unwrap(), restored.digest().unwrap());
}

#[test]
fn resolved_en_input_keeps_exact_locators_and_never_uses_the_jp_fallback() {
    let fixture = Fixture::new();
    let (mut rules, mut identity) = project(&fixture);
    rules["scope"]["region"] = json!("en");
    for title in rules["titles"].as_array_mut().unwrap() {
        title["anchor"]["region"] = json!("en");
    }
    for resource in rules["resources"].as_array_mut().unwrap() {
        resource["names"][0]["region"] = json!("en");
        if resource.get("template").is_some() {
            resource["template"]["region"] = json!("en");
        }
    }
    if let EngineIdentityInput::Resolved { region, .. } = &mut identity {
        *region = "en".into();
    }
    Catalog::from_documents_with_rules(
        &fixture.snapshot(),
        &fixture.keywords.to_string(),
        &fixture.documents(),
        &rules.to_string(),
        &identity,
    )
    .unwrap();
    Catalog::from_documents_with_rules(
        &fixture.snapshot(),
        &fixture.keywords.to_string(),
        &fixture.documents(),
        &rules.to_string(),
        &EngineIdentityInput::LegacyJp,
    )
    .unwrap_err();
    if let EngineIdentityInput::Resolved { faces, .. } = &mut identity {
        faces[5].title_code = Some("synthetic_ub".into());
    }
    Catalog::from_documents_with_rules(
        &fixture.snapshot(),
        &fixture.keywords.to_string(),
        &fixture.documents(),
        &rules.to_string(),
        &identity,
    )
    .unwrap_err();
}

#[test]
fn equal_display_labels_cannot_hide_conflicting_permanent_name_ids() {
    let mut fixture = Fixture::new();
    fixture.facts[5]["faces"][0]["name"] = json!("synthetic-drive");
    fixture.repin();
    let (rules, identity) = project(&fixture);
    Catalog::from_documents_with_rules(
        &fixture.snapshot(),
        &fixture.keywords.to_string(),
        &fixture.documents(),
        &rules.to_string(),
        &identity,
    )
    .unwrap_err();
}

#[test]
fn unbound_selectors_reject_before_enumerating_an_empty_candidate_set() {
    let mut fixture = Fixture::new();
    for selector in [
        json!({"side":"self","zone":"ex","resource_role":"lesson_item"}),
        json!({"from":{"side":"self","zone":"ex"},"resource_role":"lesson_item"}),
    ] {
        fixture.programs["cards"]["actor"]["abilities"] = json!([{"kind":"activated","line":1_u8,"targets":[{"key":"1","select":selector,"min":1_u8,"max":1_u8}],"body":{"op":"draw","count":0_u8}}]);
        let unbound = Catalog::from_documents(
            &fixture.snapshot(),
            &fixture.keywords.to_string(),
            &fixture.documents(),
        )
        .unwrap();
        let game = from_catalog(Arc::new(unbound), &position()).unwrap();
        assert!(matches!(game.legal(), Err(EngineFailure::Unsupported(_))));
    }
}
