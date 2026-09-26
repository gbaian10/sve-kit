//! Synthetic cards exercise rules independently of printed-card identities.

extern crate alloc;

use alloc::sync::Arc;

use serde_json::{Value, json};
use sve_engine::EngineFailure;
use sve_engine::catalog::Catalog;
use sve_engine::game::{Game, View};

fn snapshot() -> String {
    [("unit-follower","フォロワー"),("unit-spell","スペル")].iter().map(|(number,kind)|json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n")
}
fn registry() -> String {
    json!({"version":"astra/1","keywords":{"guard":{"ja":"守護","rule":"12.8","expansion":{"op":"keyword","name":"guard"}}}}).to_string()
}
fn document(body: &Value) -> Value {
    json!({"version":"astra/1","cards":{"unit-follower":{"status":"complete","review":"synthetic","abilities":[]},"unit-spell":{"status":"complete","review":"synthetic","abilities":[{"kind":"spell","line":1,"targets":[{"key":"1","select":{"zone":"field","side":"opponent","type":"follower"},"min":1,"max":1}],"body":body}]}}})
}
#[expect(
    clippy::unwrap_used,
    reason = "A synthetic fixture construction error must fail its test."
)]
fn catalog(body: &Value) -> Catalog {
    Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("unit.yaml".into(), document(body).to_string())],
    )
    .unwrap()
}
fn setup() -> Value {
    json!({"turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":3,"P2":2},"phase":"main"},"players":{"P1":{"leader":{"class":"ロイヤル","life":20},"pp":{"current":2,"max":2},"ep":0,"sep":0,"construction":"class","zones":{"field":[{"id":"a","card":"unit-follower"}],"hand":[{"id":"s","card":"unit-spell"}]}},"P2":{"leader":{"class":"ウィッチ","life":20},"pp":{"current":0,"max":2},"ep":0,"sep":0,"construction":"class","zones":{"field":[{"id":"b","card":"unit-follower"}]}}}})
}
#[expect(
    clippy::unwrap_used,
    reason = "A synthetic fixture construction error must fail its test."
)]
fn game(body: &Value) -> Game {
    Game::new(
        Arc::new(catalog(body)),
        &setup(),
        &Value::Null,
        &Value::Null,
        "unit-seed",
    )
    .unwrap()
}

#[test]
fn loader_rejects_invalid_grammar_duplicates_and_macro_cycles() {
    let valid = document(&json!({"op":"damage","subjects":"target.1","amount":2_i64}));
    let mut bad = valid.clone();
    bad["cards"]["unit-spell"]["abilities"][0]["body"] =
        json!({"op":"damage","subjects":"target.1"});
    Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("bad.yaml".into(), bad.to_string())],
    )
    .unwrap_err();
    let mut unknown = valid.clone();
    unknown["cards"]["unit-spell"]["abilities"][0]["body"] = json!({"op":"macro","name":"missing"});
    Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("bad.yaml".into(), unknown.to_string())],
    )
    .unwrap_err();
    let mut cycle = valid.clone();
    cycle["cards"]["unit-spell"]["abilities"][0]["body"] = json!({"op":"macro","name":"guard"});
    let registry_cycle = json!({"version":"astra/1","keywords":{"guard":{"ja":"守護","expansion":{"op":"macro","name":"guard"}}}});
    Catalog::from_documents(
        &snapshot(),
        &registry_cycle.to_string(),
        &[("cycle.yaml".into(), cycle.to_string())],
    )
    .unwrap_err();
    let docs = vec![
        ("first.yaml".into(), valid.to_string()),
        ("second.yaml".into(), valid.to_string()),
    ];
    Catalog::from_documents(&snapshot(), &registry(), &docs).unwrap_err();
    Catalog::from_documents(&format!("{}\n{}", snapshot(), snapshot()), &registry(), &[])
        .unwrap_err();
}

#[test]
fn illegal_and_unsupported_decisions_rollback_every_state_component() {
    for body in [
        json!({"op":"damage","subjects":"target.1","amount":2_i64}),
        json!({"op":"unsupported","reason":"synthetic unsupported frontier"}),
    ] {
        let mut engine = game(&body);
        let before = engine.digest().unwrap();
        engine.decide(&json!(false), "malformed").unwrap_err();
        assert_eq!(engine.digest().unwrap(), before);
        let denied = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["a"]}}),
                "illegal-target",
            )
            .unwrap();
        assert_eq!(denied.outcome, "cannot-play");
        assert_eq!(engine.digest().unwrap(), before);
        let wrong_window = engine
            .decide(&json!({"do":"pass"}), "wrong-window")
            .unwrap();
        assert_eq!(wrong_window.outcome, "cannot-play");
        assert_eq!(engine.digest().unwrap(), before);
        let result = engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "valid-target",
        );
        if let Err(error) = result {
            assert!(matches!(error, EngineFailure::Unsupported(_)));
            assert_eq!(engine.digest().unwrap(), before);
        } else {
            assert_eq!(
                engine.query(View::P1, "P2.field.b.hp").unwrap(),
                Some(json!(1_i64))
            );
        }
    }
}

#[test]
fn partial_programs_fail_closed_and_unseen_hand_has_no_identity() {
    let mut partial = document(&json!({"op":"draw","count":1_i64}));
    partial["cards"]["unit-spell"]["status"] = json!("partial");
    let catalog = Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("partial.yaml".into(), partial.to_string())],
    )
    .unwrap();
    assert_eq!(catalog.authored_count(), 2);
    let partial_engine = Game::new(
        Arc::new(catalog),
        &setup(),
        &Value::Null,
        &Value::Null,
        "unit",
    )
    .unwrap();
    assert!(matches!(
        partial_engine.legal(),
        Err(EngineFailure::Unsupported(_))
    ));
    let engine = game(&json!({"op":"damage","subjects":"target.1","amount":2_i64}));
    let packet = engine.projection(View::P2).unwrap();
    assert!(packet["objects"].get("s").is_none());
    assert_eq!(packet["P1"]["hand"], json!([{"filler":1_i64}]));
    assert_eq!(packet["awaiting"], json!({"by":"P1"}));
    assert!(packet.get("legal").is_none());
    assert!(engine.query(View::P2, "P1.hand.s.card").unwrap().is_none());
}

#[test]
fn resolution_counts_survive_a_pause_and_serialized_resume() {
    let grave = json!({"count":{"zone":"cemetery","side":"self"}});
    let mut engine = game(&json!({"op":"seq","steps":[
        {"op":"move","subjects":"self","to":"cemetery"},
        {"op":"optional","then":{"op":"seq","steps":[
            {"op":"damage","subjects":"target.1","amount":{"at":"resolution-start","value":grave}},
            {"op":"damage","subjects":"target.1","amount":grave}
        ]}}
    ]}));
    let step = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(step.outcome, "paused");
    let bytes = serde_json::to_string(&engine).unwrap();
    let mut restored: Game = serde_json::from_str(&bytes).unwrap();
    restored
        .decide(&json!({"do":"resolve-choice","choice":"execute"}), "resume")
        .unwrap();
    assert_eq!(
        restored.query(View::P1, "P2.field.b.hp").unwrap(),
        Some(json!(2_i64))
    );
}

#[test]
fn bound_selections_preserve_owner_and_values_support_aggregation() {
    let mut engine = game(
        &json!({"op":"damage","subjects":{"from":"target.1","type":"follower"},"amount":{"fn":"sum","args":[{"values":{"zone":"field","side":"self"},"field":"cost"}]}}),
    );
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.field.b.hp").unwrap(),
        Some(json!(2_i64))
    );
}

#[test]
fn combo_counts_the_card_currently_being_played() {
    let mut engine = game(
        &json!({"op":"choice","min":{"if":{"fn":"eq","args":[{"read":"self.combo"},1_i64]},"then":1_i64,"else":0_i64},"max":1_i64,"modes":[{"op":"damage","subjects":"target.1","amount":{"read":"self.combo"}}]}),
    );
    let legal = engine.legal().unwrap();
    assert!(
        legal
            .iter()
            .any(|choice| choice["do"] == "play" && choice["options"] == json!([1_i64]))
    );
    assert!(
        !legal
            .iter()
            .any(|choice| choice["do"] == "play" && choice["options"] == json!([]))
    );
    engine
        .decide(
            &json!({"do":"play","card":"s","options":[1_i64],"targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.field.b.hp").unwrap(),
        Some(json!(2_i64))
    );
}

#[test]
fn movement_receipts_count_changes_and_bind_the_actual_results() {
    for (destination, expected) in [("field", 20_i64), ("cemetery", 19_i64)] {
        let mut engine = game(
            &json!({"op":"if_done","attempt":{"op":"move","subjects":"target.1","to":destination,"bind":"moved"},"then":{"op":"damage","subjects":"opponent.leader","amount":{"count":"moved"}}}),
        );
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(expected))
        );
    }
}

#[test]
fn a_card_returning_to_the_field_is_not_the_original_target() {
    let mut engine = game(&json!({"op":"seq","steps":[
        {"op":"move","subjects":"target.1","to":"cemetery","bind":"left"},
        {"op":"move","subjects":"left","to":"field"},
        {"op":"damage","subjects":"target.1","amount":2_i64},
        {"op":"damage","subjects":"target.1.leader","amount":1_i64}
    ]}));
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.field.b.hp").unwrap(),
        Some(json!(3_i64))
    );
    assert_eq!(
        engine.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(19_i64))
    );
}

#[test]
fn targeting_protection_and_zero_damage_bane_follow_distinct_rules() {
    for keyword in ["aura", "intimidate", "bane"] {
        let mut doc = document(&json!({"op":"damage","subjects":"target.1","amount":2_i64}));
        doc["cards"]["unit-follower"]["abilities"] =
            json!([{"kind":"static","line":1_i64,"body":{"op":"keyword","name":keyword}}]);
        let words = json!({"version":"astra/1","keywords":{keyword:{"ja":keyword,"expansion":{"op":"keyword","name":keyword}}}});
        let loaded = Catalog::from_documents(
            &snapshot(),
            &words.to_string(),
            &[("unit.yaml".into(), doc.to_string())],
        )
        .unwrap();
        let mut position = setup();
        position["players"]["P1"]["zones"]["field"][0]["state"] = json!({"power":0_i64});
        position["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut engine = Game::new(
            Arc::new(loaded),
            &position,
            &Value::Null,
            &Value::Null,
            "keywords",
        )
        .unwrap();
        let legal = engine.legal().unwrap();
        if keyword == "aura" {
            assert!(!legal.iter().any(|choice| choice["do"] == "play"));
            assert!(
                legal
                    .iter()
                    .any(|choice| choice["do"] == "attack" && choice["target"] == "b")
            );
            assert_eq!(
                engine
                    .decide(
                        &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                        "aura"
                    )
                    .unwrap()
                    .outcome,
                "cannot-play"
            );
        } else if keyword == "intimidate" {
            assert!(
                !legal
                    .iter()
                    .any(|choice| choice["do"] == "attack" && choice["target"] == "b")
            );
            assert!(legal.iter().any(|choice| choice["do"] == "play"));
        } else {
            engine
                .decide(
                    &json!({"do":"attack","attacker":"a","target":"b"}),
                    "attack",
                )
                .unwrap();
            engine.decide(&json!({"do":"pass"}), "quick-pass").unwrap();
            assert_eq!(
                engine.query(View::P1, "P2.cemetery").unwrap(),
                Some(json!(["b"]))
            );
        }
    }
}

#[test]
fn evolution_enumerates_resources_and_preserves_existing_damage() {
    let mut doc = document(&json!({"op":"damage","subjects":"target.1","amount":2_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{"kind":"evolve","line":1_i64,"costs":[{"op":"pp","amount":1_i64}],"body":{"op":"evolve","subjects":"self"}}]);
    doc["cards"]["unit-evolved"] = json!({"status":"complete","review":"synthetic","abilities":[]});
    let evolved = json!({"number":"unit-evolved","faces":[{"name":"unit-follower","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"1","power":"3","hp":"5","traits":[],"text":null,"sections":[]}]});
    let facts = format!("{}\n{}", snapshot(), evolved);
    let catalog = Catalog::from_documents(
        &facts,
        &registry(),
        &[("unit.yaml".into(), doc.to_string())],
    )
    .unwrap();
    let mut position = setup();
    position["turn"]["elapsed_turns"]["P1"] = json!(7_i64);
    position["players"]["P1"]["ep"] = json!(1_i64);
    position["players"]["P1"]["sep"] = json!(1_i64);
    position["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"e","card":"unit-evolved"}]);
    position["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"hp":1_i64,"max_hp":3_i64,"damage":2_i64});
    let engine = Game::new(
        Arc::new(catalog.clone()),
        &position,
        &Value::Null,
        &Value::Null,
        "evolution",
    )
    .unwrap();
    let choices = engine
        .legal()
        .unwrap()
        .into_iter()
        .filter(|choice| choice["do"] == "evolve")
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 4);
    for choice in &choices {
        let mut copy = engine.clone();
        let result = copy.decide(choice, "evolve").unwrap();
        assert_eq!(result.outcome, "resolved");
        let extra = choice["pay"]["sep"].as_i64().unwrap();
        assert_eq!(
            copy.query(View::P1, "P1.field.a.hp").unwrap(),
            Some(json!(3_i64.saturating_add(extra)))
        );
        assert_eq!(
            copy.query(View::P1, "P1.field.a.max_hp").unwrap(),
            Some(json!(5_i64.saturating_add(extra)))
        );
        assert!(
            !copy
                .legal()
                .unwrap()
                .iter()
                .any(|candidate| candidate["do"] == "evolve")
        );
    }
    position["turn"]["elapsed_turns"]["P1"] = json!(6_i64);
    let mut early = Game::new(
        Arc::new(catalog),
        &position,
        &Value::Null,
        &Value::Null,
        "early",
    )
    .unwrap();
    assert_eq!(
        early
            .legal()
            .unwrap()
            .iter()
            .filter(|choice| choice["do"] == "evolve")
            .count(),
        2
    );
    let before = early.digest().unwrap();
    let premature = choices
        .iter()
        .find(|choice| choice["pay"]["sep"] == 1_i64)
        .unwrap();
    assert_eq!(
        early.decide(premature, "too-early").unwrap().outcome,
        "cannot-evolve"
    );
    assert_eq!(early.digest().unwrap(), before);
}

#[test]
fn activated_limits_and_x_domains_survive_observation_sampling() {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{
        "kind":"activated","line":1_i64,"limit":1_i64,
        "variables":{"x":{"min":0_i64,"max":2_i64}},
        "costs":[{"op":"pp","amount":{"read":"x"}}],
        "body":{"op":"damage","subjects":"opponent.leader","amount":{"read":"x"}}
    }]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("limits.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut engine = Game::new(
        Arc::clone(&catalog),
        &setup(),
        &Value::Null,
        &Value::Null,
        "limit",
    )
    .unwrap();
    let options = engine.legal().unwrap();
    assert_eq!(
        options
            .iter()
            .filter(|choice| choice["do"] == "activate")
            .count(),
        3
    );
    let before = engine.digest().unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64},"x":3_i64}),
                "bad-x"
            )
            .unwrap()
            .outcome,
        "cannot-activate"
    );
    assert_eq!(engine.digest().unwrap(), before);
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":1_i64},"x":2_i64}),
            "valid-x",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(18_i64))
    );
    assert!(
        engine
            .legal()
            .unwrap()
            .iter()
            .all(|choice| choice["do"] != "activate")
    );
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(
        packet["semantic_state"]["used_this_turn"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    let sampled = Game::from_observation(catalog, &packet, "P1", "sample").unwrap();
    assert_eq!(sampled.legal().unwrap(), engine.legal().unwrap());
}

#[test]
fn cost_free_pending_cannot_be_declined() {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{"kind":"trigger","line":1_i64,"event":"attack","body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}}]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("pending.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut engine = Game::new(catalog, &setup(), &Value::Null, &Value::Null, "pending").unwrap();
    engine
        .decide(
            &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
            "attack",
        )
        .unwrap();
    let mut decision = engine.legal().unwrap().into_iter().next().unwrap();
    assert_eq!(decision["do"], "choose-pending");
    let before = engine.digest().unwrap();
    decision["costs"] = json!("decline");
    engine.decide(&decision, "decline").unwrap_err();
    assert_eq!(engine.digest().unwrap(), before);
}

#[test]
fn delayed_end_and_temporary_silence_survive_snapshot_and_observation() {
    let body = json!({"op":"seq","steps":[
        {"op":"modify","subjects":"target.1","remove_abilities":true,"until":"end-of-turn"},
        {"op":"delay","event":"end","once":true,"body":{"op":"damage","subjects":"opponent.leader","amount":3_i64}}
    ]});
    let catalog = Arc::new(catalog(&body));
    let mut engine = Game::new(
        Arc::clone(&catalog),
        &setup(),
        &Value::Null,
        &Value::Null,
        "temporal",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.field.b.silenced").unwrap(),
        Some(json!(true))
    );
    let packet = engine.projection(View::P1).unwrap();
    let mut sampled =
        Game::from_observation(Arc::clone(&catalog), &packet, "P1", "sample").unwrap();
    sampled.decide(&json!({"do":"end-phase"}), "end").unwrap();
    engine.decide(&json!({"do":"end-phase"}), "end").unwrap();
    assert_eq!(sampled.legal().unwrap(), engine.legal().unwrap());
    let pending = engine.projection(View::P1).unwrap();
    sampled = Game::from_observation(catalog, &pending, "P1", "pending-sample").unwrap();
    let decision = sampled.legal().unwrap().into_iter().next().unwrap();
    assert_eq!(decision["do"], "choose-pending");
    sampled.decide(&decision, "resolve").unwrap();
    assert_eq!(
        sampled.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(17_i64))
    );
    sampled.decide(&json!({"do":"pass"}), "pass").unwrap();
    assert_eq!(
        sampled.query(View::P1, "P2.field.b.silenced").unwrap(),
        Some(json!(false))
    );
    assert_eq!(
        sampled.projection(View::P1).unwrap()["semantic_state"]["delayed_triggers"],
        json!([])
    );
}

#[test]
fn unsupported_modifier_parameters_cannot_succeed_silently() {
    let mut engine = game(&json!({"op":"modify","subjects":"target.1","abilities":[]}));
    let before = engine.digest().unwrap();
    let error = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "unsupported",
        )
        .unwrap_err();
    assert!(matches!(error, EngineFailure::Unsupported(_)));
    assert_eq!(engine.digest().unwrap(), before);
}

#[test]
fn spell_cost_enumeration_and_payment_include_the_printed_cost() {
    let mut doc = document(&json!({"op":"damage","subjects":"target.1","amount":1_i64}));
    doc["cards"]["unit-spell"]["abilities"][0]["costs"] = json!([{"op":"pp","amount":2_i64}]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("spell-cost.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut position = setup();
    for (available, legal) in [(2_i64, false), (3_i64, true)] {
        position["players"]["P1"]["pp"]["current"] = json!(available);
        let mut engine = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &Value::Null,
            "payment",
        )
        .unwrap();
        assert_eq!(
            engine
                .legal()
                .unwrap()
                .iter()
                .any(|action| action["do"] == "play"),
            legal
        );
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        assert_eq!(step.outcome, if legal { "resolved" } else { "cannot-play" });
        assert_eq!(
            engine.query(View::P1, "P1.pp.current").unwrap(),
            Some(json!(if legal { 0 } else { available }))
        );
    }
}

#[test]
fn life_decreases_are_distinct_from_damage_and_stat_changes_are_per_object() {
    let mut engine = game(&json!({"op":"seq","steps":[
        {"op":"modify","subjects":"self.leader","hp":-3_i64},
        {"op":"modify","subjects":"self.leader","hp":2_i64},
        {"op":"modify","subjects":"self.leader","set_hp":15_i64},
        {"op":"damage","subjects":"opponent.leader","amount":{"read":"self.turn.leader_hp_decreased"}},
        {"op":"modify","subjects":"target.1","power":1_i64,"hp":1_i64}
    ]}));
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(packet["P1"]["leader"]["life"], 15_i64);
    assert_eq!(packet["P2"]["leader"]["life"], 18_i64);
    assert_eq!(
        packet["semantic_state"]["counters_this_turn"]["P1.leader_hp_decreased"],
        2_i64
    );
    assert_eq!(
        packet["semantic_state"]["counters_this_turn"]["P1.leader_hp_increased"],
        2_i64
    );
    assert_eq!(
        packet["semantic_state"]["counters_this_turn"]["P1.leader_damaged"],
        0_i64
    );
    assert_eq!(packet["objects"]["b"]["stats_increased_this_turn"], true);
    assert_eq!(packet["objects"]["a"]["stats_increased_this_turn"], false);
    engine.decide(&json!({"do":"end-phase"}), "end").unwrap();
    engine.decide(&json!({"do":"pass"}), "pass").unwrap();
    assert_eq!(
        engine.projection(View::P1).unwrap()["objects"]["b"]["stats_increased_this_turn"],
        false
    );
}

#[test]
fn localized_keyword_and_counter_packets_restore_stable_identifiers() {
    let mut registry: Value = serde_json::from_str(&registry()).unwrap();
    registry["keywords"]["stack_counter"] = json!({"ja":"スタックカウンター","role":"counter"});
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry.to_string(),
            &[(
                "stable.yaml".into(),
                document(&json!({"op":"draw","count":0_i64})).to_string(),
            )],
        )
        .unwrap(),
    );
    let mut position = setup();
    position["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"keywords":["守護"],"counters":{"スタックカウンター":2_i64}});
    let engine = Game::new(
        Arc::clone(&catalog),
        &position,
        &Value::Null,
        &Value::Null,
        "stable",
    )
    .unwrap();
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(packet["objects"]["a"]["keywords"], json!(["守護"]));
    assert_eq!(
        packet["objects"]["a"]["counters"]["スタックカウンター"],
        2_i64
    );
    let sampled = Game::from_observation(catalog, &packet, "P1", "sample").unwrap();
    assert_eq!(
        sampled.projection(View::P1).unwrap()["objects"],
        packet["objects"]
    );
    let serialized = serde_json::to_value(sampled).unwrap();
    assert_eq!(
        serialized["state"]["objects"]["a"]["state"]["keywords"],
        json!(["guard"])
    );
    assert_eq!(
        serialized["state"]["objects"]["a"]["state"]["counters"]["stack_counter"],
        2_i64
    );
}

#[test]
fn last_words_trigger_once_for_destroy_and_for_payment_movement() {
    for payment in [false, true] {
        let body = if payment {
            json!({"op":"seq","steps":[]})
        } else {
            json!({"op":"destroy","subjects":"target.1"})
        };
        let mut doc = document(&body);
        doc["cards"]["unit-follower"]["abilities"] = json!([{
            "kind":"trigger","line":1_i64,"event":"field_to_cemetery","subject":"self",
            "body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}
        }]);
        if payment {
            doc["cards"]["unit-spell"]["abilities"][0]["costs"] =
                json!([{"op":"move","subjects":"target.1","to":"cemetery"}]);
        }
        let catalog = Arc::new(
            Catalog::from_documents(
                &snapshot(),
                &registry(),
                &[("last-words.yaml".into(), doc.to_string())],
            )
            .unwrap(),
        );
        let mut engine =
            Game::new(catalog, &setup(), &Value::Null, &Value::Null, "last-words").unwrap();
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        let pending = engine.legal().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["do"], "choose-pending");
        engine.decide(&pending[0], "last-words").unwrap();
        assert_eq!(
            engine.query(View::P1, "P1.leader.life").unwrap(),
            Some(json!(19_i64))
        );
        assert!(
            engine
                .legal()
                .unwrap()
                .iter()
                .all(|action| action["do"] != "choose-pending")
        );
    }
}
