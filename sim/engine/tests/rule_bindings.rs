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
use sve_engine::replay::Replay;

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
        let mut programs = json!({"version":"astra/1","cards":{}});
        for (number, ..) in definitions {
            programs["cards"][number] =
                json!({"status":"complete","review":"synthetic","abilities":[]});
        }
        programs["cards"]["lesson"]["rules_name"] = json!("synthetic-lesson");
        programs["cards"]["alias"]["abilities"] = json!([{"kind":"static","line":1_u8,"body":{"op":"name_alias","subjects":"self","name":"synthetic-lesson","while_zone":"any"}}]);
        let mut fixture = Self {
            facts,
            programs,
            keywords: json!({"version":"astra/1","keywords":{}}),
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
fn loader_background_and_player_fields_are_required_for_same_version_saves() {
    let fixture = Fixture::new();
    let mut setup = position();
    setup["players"]["P1"]["title_code"] = json!("synthetic_empty");
    let game = fixture.game(&setup).unwrap();
    let mut saved = serde_json::to_value(&game).unwrap();
    let restored: Game = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(restored.digest().unwrap(), game.digest().unwrap());
    assert_eq!(restored.legal().unwrap(), game.legal().unwrap());
    saved["catalog"]
        .as_object_mut()
        .unwrap()
        .remove("rule_bindings");
    serde_json::from_value::<Game>(saved).unwrap_err();
    let mut player_missing = serde_json::to_value(&game).unwrap();
    player_missing["state"]["players"]["P1"]
        .as_object_mut()
        .unwrap()
        .remove("title_code");
    serde_json::from_value::<Game>(player_missing).unwrap_err();
    let projection = game.projection(View::P2).unwrap().to_string();
    for private in [
        "settings_sha256",
        "identity_sha256",
        "name_labels",
        "rule_bindings",
    ] {
        assert!(!projection.contains(private));
    }
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
    create_dir_all(root.join("effects")).unwrap();
    create_dir_all(root.join("engine-rules")).unwrap();
    write(root.join("cards.jsonl"), fixture.snapshot()).unwrap();
    write(root.join("keywords.yaml"), fixture.keywords.to_string()).unwrap();
    write(root.join("effects/unit.yaml"), fixture.programs.to_string()).unwrap();
    write(
        root.join("engine-rules/index.yaml"),
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
fn five_opening_tokens_require_title_mode_and_the_bound_capability() {
    let fixture = Fixture::new();
    let catalog = Arc::new(fixture.load().unwrap());
    for construction in ["title", "class", "crossover"] {
        for two_players in [false, true] {
            let mut setup = json!({"pregame":true,"players":{}});
            for player in ["P1", "P2"] {
                let enabled = player == "P1" || two_players;
                setup["players"][player] = json!({"construction":construction,"title_code":if enabled {"synthetic_lesson"} else {"synthetic_empty"},"leader":{"class":"ニュートラル","life":20_u8},"deck_list":[],"evolve_deck_list":[]});
            }
            let game = from_catalog(Arc::clone(&catalog), &setup).unwrap();
            for player in ["P1", "P2"] {
                let count = if construction == "title" && (player == "P1" || two_players) {
                    5
                } else {
                    0
                };
                assert_eq!(
                    game.query(View::Referee, &format!("{player}.ex"))
                        .unwrap()
                        .unwrap()
                        .as_array()
                        .unwrap()
                        .len(),
                    count
                );
            }
        }
    }
}

#[test]
fn ub_requires_title_capability_and_keeps_controller_and_active_zone_checks() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    let catalog = Arc::new(fixture.load().unwrap());
    for construction in ["title", "class", "crossover"] {
        for code in ["synthetic_ub", "synthetic_empty"] {
            let mut setup = position();
            setup["players"]["P1"]["construction"] = json!(construction);
            setup["players"]["P1"]["title_code"] = json!(code);
            let game = from_catalog(Arc::clone(&catalog), &setup).unwrap();
            assert_eq!(
                !choices(&game, 4).is_empty(),
                construction == "title" && code == "synthetic_ub"
            );
            setup["players"]["P1"]["zones"]["field"] = json!([]);
            setup["players"]["P1"]["zones"]["hand"] = json!([{"id":"actor","card":"actor"}]);
            assert!(choices(&from_catalog(Arc::clone(&catalog), &setup).unwrap(), 4).is_empty());
        }
    }
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
    assert!(choices(&from_catalog(Arc::clone(&catalog), &setup).unwrap(), 1).is_empty());
}

#[test]
fn banish_counter_uses_only_the_single_post_move_name_and_previous_controller() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    let catalog = Arc::new(fixture.load().unwrap());
    for (zone, card, expected) in [
        ("ex", "lesson", 1_i64),
        ("field", "lesson", 0),
        ("ex", "alias", 0),
        ("ex", "meal", 0),
    ] {
        let mut setup = position();
        setup["players"]["P2"]["zones"][zone] = json!([{"id":"target","card":card}]);
        let mut game = from_catalog(Arc::clone(&catalog), &setup).unwrap();
        game.decide(&json!({"do":"activate","ability":{"source":"actor","line":5_u8},"targets":{"1":["target"]}}), "banish").unwrap();
        let counters =
            game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"].clone();
        assert_eq!(
            counters["P2"]["magic_item_banished"].as_i64().unwrap_or(0),
            expected
        );
        assert_eq!(
            counters["P1"]["magic_item_banished"].as_i64().unwrap_or(0),
            0
        );
    }
    let mut setup = position();
    setup["players"]["P1"]["zones"]["ex"] = json!([{"id":"target","card":"lesson"}]);
    let mut game = from_catalog(Arc::clone(&catalog), &setup).unwrap();
    game.decide(&json!({"do":"activate","ability":{"source":"actor","line":7_u8},"targets":{"1":["target"]}}), "other-destination").unwrap();
    assert!(game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]["magic_item_banished"].is_null());
}

#[test]
fn selector_name_and_role_are_an_intersection_in_both_object_forms() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    for from in [false, true] {
        for name in [
            "synthetic-lesson",
            "synthetic-printed-resource",
            "synthetic-alias",
        ] {
            let selector = if from {
                json!({"from":{"side":"self","zone":"ex"},"resource_role":"lesson_item","name":name})
            } else {
                json!({"side":"self","zone":"ex","resource_role":"lesson_item","name":name})
            };
            fixture.programs["cards"]["actor"]["abilities"] = json!([{"kind":"activated","line":1_u8,"targets":[{"key":"1","select":selector,"min":1_u8,"max":1_u8}],"body":{"op":"draw","count":0_u8}}]);
            let mut setup = position();
            setup["players"]["P1"]["zones"]["ex"] = json!([{"id":"target","card":"lesson"}]);
            assert_eq!(
                !choices(&fixture.game(&setup).unwrap(), 1).is_empty(),
                name == "synthetic-lesson"
            );
        }
    }
}

#[test]
fn consistent_display_renaming_preserves_ids_capabilities_and_cost_results() {
    let mut original = Fixture::new();
    resource_programs(&mut original);
    let mut renamed = original.clone();
    for card in &mut renamed.facts {
        for face in card["faces"].as_array_mut().unwrap() {
            for field in ["name", "title"] {
                let label = face[field].as_str().unwrap();
                if !label.is_empty() {
                    face[field] = json!(format!("renamed-{label}"));
                }
            }
        }
    }
    renamed.programs["cards"]["lesson"]["rules_name"] = json!("renamed-synthetic-lesson");
    renamed.programs["cards"]["alias"]["abilities"][0]["body"]["name"] =
        json!("renamed-synthetic-lesson");
    renamed.repin();
    let mut setup = position();
    setup["players"]["P1"]["title_code"] = json!("synthetic_ub");
    setup["players"]["P1"]["construction"] = json!("title");
    setup["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"lesson","card":"lesson"},{"id":"alias","card":"alias"}]);
    let mut before = original.game(&setup).unwrap();
    let mut after = renamed.game(&setup).unwrap();
    assert_eq!(before.legal().unwrap(), after.legal().unwrap());
    let action = choices(&before, 1).remove(0);
    assert_eq!(
        before.decide(&action, "same").unwrap().outcome,
        after.decide(&action, "same").unwrap().outcome
    );
    for path in [
        "P1.ex",
        "P1.pp.current",
        "semantic_state.counters_this_turn",
    ] {
        assert_eq!(
            before.query(View::Referee, path).unwrap(),
            after.query(View::Referee, path).unwrap()
        );
    }
    let stack_action = choices(&before, 6).remove(0);
    before.decide(&stack_action, "stack").unwrap();
    after.decide(&stack_action, "stack").unwrap();
    assert_eq!(
        before.query(View::Referee, "P1.field").unwrap(),
        after.query(View::Referee, "P1.field").unwrap()
    );
    assert_eq!(
        before
            .query(View::Referee, "P1.field.new-1.counters")
            .unwrap(),
        after
            .query(View::Referee, "P1.field.new-1.counters")
            .unwrap()
    );
    let opening_setup = json!({"pregame":true,"players":{
        "P1":{"construction":"title","title_code":"synthetic_lesson","leader":{"life":20_u8},"deck_list":[],"evolve_deck_list":[]},
        "P2":{"construction":"class","leader":{"life":20_u8},"deck_list":[],"evolve_deck_list":[]}}});
    let opening_before = original.game(&opening_setup).unwrap();
    let opening_after = renamed.game(&opening_setup).unwrap();
    assert_eq!(
        opening_before.query(View::Referee, "P1.ex").unwrap(),
        opening_after.query(View::Referee, "P1.ex").unwrap()
    );
    assert_eq!(
        opening_before.legal().unwrap(),
        opening_after.legal().unwrap()
    );
}

#[test]
fn counter_queries_the_reset_face_and_information_source_after_moving() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    // Movement clears temporary face/source state before this existing counter is read.
    fixture.programs["cards"]["lesson"]
        .as_object_mut()
        .unwrap()
        .remove("rules_name");
    fixture.rules["resources"][0]["names"][0]["name_source"] = json!("printed");
    let mut extra = fixture.facts[1]["faces"][0].clone();
    extra["name"] = json!("synthetic-other-face");
    fixture.facts[1]["faces"]
        .as_array_mut()
        .unwrap()
        .push(extra);
    fixture.repin();
    for (card, state, expected) in [
        ("lesson", json!({"face":1_u8}), 1_i64),
        ("alias", json!({"evolved_with":"source"}), 0),
    ] {
        let mut setup = position();
        setup["players"]["P2"]["zones"]["ex"] = json!([{"id":"target","card":card,"state":state}]);
        setup["players"]["P2"]["zones"]["evolution"] = json!([{"id":"source","card":"lesson"}]);
        let mut game = fixture.game(&setup).unwrap();
        game.decide(&json!({"do":"activate","ability":{"source":"actor","line":5_u8},"targets":{"1":["target"]}}), "reset-name").unwrap();
        assert_eq!(game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P2"]["magic_item_banished"].as_i64().unwrap_or(0), expected);
    }
    for (override_name, expected) in [
        ("synthetic-printed-resource", 1_i64),
        ("synthetic-wrong", 0),
    ] {
        fixture.programs["cards"]["alias"]["rules_name"] = json!(override_name);
        let mut setup = position();
        setup["players"]["P1"]["zones"]["ex"] = json!([{"id":"target","card":"alias"}]);
        let mut game = fixture.game(&setup).unwrap();
        game.decide(&json!({"do":"activate","ability":{"source":"actor","line":5_u8},"targets":{"1":["target"]}}), "override").unwrap();
        assert_eq!(game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]["magic_item_banished"].as_i64().unwrap_or(0), expected);
    }
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
    game.decide(&json!({"do":"activate","ability":{"source":"actor","line":5_u8},"targets":{"1":["target"]}}), "unbound").unwrap_err();
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
fn same_version_replay_restores_pending_stack_and_preserves_branches_and_visibility() {
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
    let (mut replay, root) = Replay::start(fixture.game(&setup).unwrap(), "bindings");
    let (pending, step) = replay
        .decide(
            &root,
            &json!({"do":"activate","ability":{"source":"actor","line":6_u8}}),
        )
        .unwrap();
    assert_eq!(step.outcome, "paused");
    let blob = replay.save(&pending).unwrap();
    let (mut restored, selected) = Replay::restore(&blob).unwrap();
    assert_eq!(pending, selected);
    assert_eq!(
        replay.game(&pending).unwrap().legal().unwrap(),
        restored.game(&selected).unwrap().legal().unwrap()
    );
    let action = restored.game(&selected).unwrap().legal().unwrap().remove(0);
    let (a, original_step) = replay.decide(&pending, &action).unwrap();
    let (b, restored_step) = restored.decide(&selected, &action).unwrap();
    assert_eq!(original_step.events, restored_step.events);
    assert_eq!(
        replay.game(&a).unwrap().digest().unwrap(),
        restored.game(&b).unwrap().digest().unwrap()
    );
    let branch = restored.branch(&root, true).unwrap();
    assert_eq!(
        restored.game(&root).unwrap().legal().unwrap(),
        restored.game(&branch).unwrap().legal().unwrap()
    );
    assert!(
        !restored
            .game(&branch)
            .unwrap()
            .projection(View::P1)
            .unwrap()
            .to_string()
            .contains("private-opponent-id")
    );
}

#[test]
fn alias_zone_and_information_source_use_the_current_effective_names() {
    let mut fixture = Fixture::new();
    resource_programs(&mut fixture);
    fixture.programs["cards"]["alias"]["abilities"][0]["body"]["while_zone"] = json!("field");
    fixture.programs["cards"]["actor"]["abilities"] = json!([{"kind":"activated","line":1_u8,"targets":[{"key":"1","select":{"side":"self","zone":"any","resource_role":"lesson_item"},"min":1_u8,"max":1_u8}],"body":{"op":"draw","count":0_u8}}]);
    let catalog = Arc::new(fixture.load().unwrap());
    for (zone, state, expected) in [
        ("field", json!({}), true),
        ("ex", json!({}), false),
        ("ex", json!({"evolved_with":"source"}), true),
    ] {
        let mut setup = position();
        let item = json!({"id":"target","card":"alias","state":state});
        if let Some(items) = setup["players"]["P1"]["zones"][zone].as_array_mut() {
            items.push(item);
        } else {
            setup["players"]["P1"]["zones"][zone] = json!([item]);
        }
        setup["players"]["P1"]["zones"]["evolution"] = json!([{"id":"source","card":"lesson"}]);
        let game = from_catalog(Arc::clone(&catalog), &setup).unwrap();
        assert_eq!(
            choices(&game, 1)
                .iter()
                .any(|action| action["targets"]["1"][0] == "target"),
            expected,
            "{zone}"
        );
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

#[test]
fn resource_template_uses_the_resolved_face_at_birth() {
    let mut fixture = Fixture::new();
    fixture.programs["cards"]["lesson"]
        .as_object_mut()
        .unwrap()
        .remove("rules_name");
    let mut second = fixture.facts[1]["faces"][0].clone();
    second["name"] = json!("synthetic-selected-face");
    fixture.facts[1]["faces"]
        .as_array_mut()
        .unwrap()
        .push(second);
    fixture.rules["resources"][0]["names"][0]["name_source"] = json!("printed");
    fixture.rules["resources"][0]["names"][0]["face"]["face_ordinal"] = json!(1_u8);
    fixture.rules["resources"][0]["template"]["face_ordinal"] = json!(1_u8);
    fixture.repin();
    let setup = json!({"pregame":true,"players":{
        "P1":{"construction":"title","title_code":"synthetic_lesson","leader":{"life":20_u8},"deck_list":[],"evolve_deck_list":[]},
        "P2":{"construction":"class","leader":{"life":20_u8},"deck_list":[],"evolve_deck_list":[]}}});
    let game = fixture.game(&setup).unwrap();
    let packet = game.projection(View::Referee).unwrap();
    let ids = packet["P1"]["ex"].as_array().unwrap();
    assert_eq!(ids.len(), 5);
    for id in ids {
        let object = &packet["objects"][id.as_str().unwrap()];
        assert_eq!(object["face"], 1_u8);
        assert_eq!(object["name"], "synthetic-selected-face");
    }
}
