//! Synthetic identities, finite capabilities and input-boundary rejection cases.

#![allow(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Fixture assertions can panic and pattern checks only handle the expected variants."
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Small synthetic fixtures fail immediately on a malformed construction or missing result."
)]

extern crate alloc;
use alloc::sync::Arc;
use serde_json::{Value, json};
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
