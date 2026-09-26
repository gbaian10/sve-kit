//! Synthetic cards exercise rules independently of printed-card identities.

extern crate alloc;

use alloc::sync::Arc;
use core::slice::from_ref;

use serde_json::{Value, json};
use sve_engine::EngineFailure;
use sve_engine::catalog::Catalog;
use sve_engine::game::{Game, View};

fn snapshot() -> String {
    [("unit-follower","フォロワー"),("unit-spell","スペル")].iter().map(|(number,kind)|json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n")
}
fn registry() -> String {
    json!({"version":"astra/1","keywords":{"guard":{"ja":"守護","rule":"12.8","expansion":{"op":"keyword","name":"guard"}},"drain":{"ja":"ドレイン","rule":"12.13","expansion":{"op":"keyword","name":"drain"}}}}).to_string()
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
    assert!(step.events.iter().all(|event| event["kind"] != "解決"));
    let bytes = serde_json::to_string(&engine).unwrap();
    let mut restored: Game = serde_json::from_str(&bytes).unwrap();
    let completed = restored
        .decide(&json!({"do":"resolve-choice","choice":"execute"}), "resume")
        .unwrap();
    let last = completed.events.last().unwrap();
    assert_eq!(last["kind"], "解決");
    assert_eq!(last["object"], "s");
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
fn pp_recovery_receipts_use_the_actual_increase_after_capping() {
    for (amount, spend, recovered) in [(0_i64, 1_i64, 0_i64), (-1, 1, 0), (3, 0, 0), (3, 1, 1)] {
        let body = json!({"op":"seq","steps":[
            {"op":"pp","amount":spend},
            {"op":"if_done","attempt":{"op":"recover_pp","amount":amount},"then":{"op":"damage","subjects":"opponent.leader","amount":1_i64}}
        ]});
        let mut doc = document(&body);
        doc["cards"]["unit-follower"]["abilities"] =
            json!([{"kind":"activated","line":1_i64,"body":body}]);
        let catalog = Arc::new(
            Catalog::from_documents(
                &snapshot(),
                &registry(),
                &[("recovery.yaml".into(), doc.to_string())],
            )
            .unwrap(),
        );
        let mut engine =
            Game::new(catalog, &setup(), &Value::Null, &Value::Null, "recovery").unwrap();
        let step = engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                "recover",
            )
            .unwrap();
        assert_eq!(step.outcome, "resolved");
        assert_eq!(
            engine.query(View::P1, "P1.pp.current").unwrap(),
            Some(json!(2_i64 - spend + recovered))
        );
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(20_i64 - recovered))
        );
        let events = step
            .events
            .iter()
            .filter(|event| event["kind"] == "回復")
            .collect::<Vec<_>>();
        assert_eq!(events.len(), usize::try_from(recovered).unwrap());
        for event in events {
            assert_eq!(event["amount"], recovered);
        }
    }
}

#[test]
fn next_spell_discount_tracks_future_cards_and_consumes_only_on_success() {
    let mut doc = document(&json!({"op":"damage","subjects":"target.1","amount":1_i64}));
    doc["cards"]["unit-spell"]["abilities"].as_array_mut().unwrap().push(json!({"kind":"static","line":2_i64,"body":{"op":"adjust_cost","subjects":"self","set":3_i64}}));
    doc["cards"]["unit-follower"]["abilities"] = json!([
        {"kind":"activated","line":1_i64,"body":{"op":"adjust_cost","subjects":{"side":"self","zone":"any","type":"spell"},"amount":-2_i64,"uses":1_i64,"consume_on":"spell_play","until":"end-of-turn"}},
        {"kind":"activated","line":2_i64,"body":{"op":"draw","count":1_i64}}
    ]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("costs.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"f","card":"unit-follower"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d","card":"unit-spell"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"e","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::clone(&catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "discount",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
            "grant",
        )
        .unwrap();
    let mut expired = engine.clone();
    expired.decide(&json!({"do":"end-phase"}), "end").unwrap();
    expired.decide(&json!({"do":"pass"}), "next").unwrap();
    assert_eq!(
        expired
            .query(View::P1, "semantic_state.continuous_effects")
            .unwrap(),
        Some(json!([]))
    );
    engine
        .decide(&json!({"do":"play","card":"f"}), "follower")
        .unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
            "draw",
        )
        .unwrap();
    let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    assert_eq!(saved.legal().unwrap(), engine.legal().unwrap());
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(
        packet["semantic_state"]["continuous_effects"][0]["applies_to"],
        json!(["P1"])
    );
    let rebuilt = Game::from_observation(catalog, &packet, "P1", "discount-world").unwrap();
    assert_eq!(rebuilt.legal().unwrap(), engine.legal().unwrap());
    let before = engine.digest().unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"play","card":"d","targets":{"1":["a"]}}),
                "invalid"
            )
            .unwrap()
            .outcome,
        "cannot-play"
    );
    assert_eq!(engine.digest().unwrap(), before);
    assert_eq!(
        engine
            .decide(
                &json!({"do":"play","card":"d","targets":{"1":["b"]}}),
                "spell"
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    assert_eq!(
        engine.query(View::P1, "P1.pp.current").unwrap(),
        Some(json!(0_i64))
    );
    assert_eq!(
        engine
            .query(View::P1, "semantic_state.continuous_effects")
            .unwrap(),
        Some(json!([]))
    );
    assert_eq!(
        engine.query(View::P1, "P2.field.b.hp").unwrap(),
        Some(json!(2_i64))
    );
}

#[test]
fn drain_captures_actual_attack_damage_and_survives_its_sources_death() {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([
        {"kind":"static","line":1_i64,"body":{"op":"keyword","name":"drain"}},
        {"kind":"static","line":2_i64,"body":{"op":"keyword","name":"drain"}},
        {"kind":"static","line":3_i64,"body":{"op":"replace_damage","subjects":"self","amount":-1_i64}},
        {"kind":"activated","line":4_i64,"body":{"op":"damage","subjects":"opponent.leader","amount":2_i64}}
    ]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("drain.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    for power in [1_i64, 2_i64] {
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"hp":1_i64,"power":power});
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut engine = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "drain",
        )
        .unwrap();
        let effect = engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":4_i64}}),
                "effect",
            )
            .unwrap();
        assert!(effect.events.iter().all(|event| event["kind"] != "待機"));
        engine
            .decide(
                &json!({"do":"attack","attacker":"a","target":"b"}),
                "attack",
            )
            .unwrap();
        let damage = engine.decide(&json!({"do":"pass"}), "damage").unwrap();
        assert_eq!(
            engine.query(View::P1, "P1.cemetery").unwrap(),
            Some(json!(["a"]))
        );
        assert_eq!(
            engine.query(View::P1, "P1.leader.life").unwrap(),
            Some(json!(20_i64))
        );
        assert_eq!(
            damage
                .events
                .iter()
                .filter(|event| event["kind"] == "待機")
                .count(),
            usize::try_from(power - 1_i64).unwrap()
        );
        if power == 1 {
            continue;
        }
        let choice = engine.legal().unwrap().into_iter().next().unwrap();
        assert_eq!(
            choice["pending"]["ability"],
            json!({"source":"a","line":1_i64,"keyword":"ドレイン","rule":"12.13.2"})
        );
        let restored: Game =
            serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
        let rebuilt = Game::from_observation(
            Arc::clone(&catalog),
            &engine.projection(View::P1).unwrap(),
            "P1",
            "drain-world",
        )
        .unwrap();
        for mut candidate in [engine, restored, rebuilt] {
            assert_eq!(
                candidate.decide(&choice, "heal").unwrap().outcome,
                "resolved"
            );
            assert_eq!(
                candidate.query(View::P1, "P1.leader.life").unwrap(),
                Some(json!(21_i64))
            );
            assert_eq!(
                candidate.query(View::P1, "P2.leader.life").unwrap(),
                Some(json!(18_i64))
            );
        }
    }
}

#[test]
fn granted_drain_keeps_the_provider_text_and_recipient_controller() {
    for aura in [false, true] {
        let mut doc = document(
            &json!({"op":"modify","subjects":{"zone":"field","side":"self"},"keywords":["drain"]}),
        );
        let mut initial = setup();
        if aura {
            doc["cards"]["unit-spell"]["abilities"] = json!([{"kind":"static","line":1_i64,"body":{"op":"aura","subjects":{"zone":"field","side":"self","type":"follower"},"keywords":["drain"]}}]);
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"provider","card":"unit-spell"}));
        }
        let catalog = Arc::new(
            Catalog::from_documents(
                &snapshot(),
                &registry(),
                &[("grants.yaml".into(), doc.to_string())],
            )
            .unwrap(),
        );
        let mut engine =
            Game::new(catalog, &initial, &Value::Null, &Value::Null, "grants").unwrap();
        if !aura {
            engine
                .decide(
                    &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                    "grant",
                )
                .unwrap();
        }
        engine
            .decide(
                &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
                "attack",
            )
            .unwrap();
        engine.decide(&json!({"do":"pass"}), "damage").unwrap();
        let choice = engine.legal().unwrap().into_iter().next().unwrap();
        assert_eq!(
            choice["pending"]["ability"],
            json!({"source":"a","card":"unit-spell","line":1_i64,"keyword":"ドレイン","rule":"12.13.2"})
        );
        assert_eq!(engine.decide(&choice, "heal").unwrap().outcome, "resolved");
        assert_eq!(
            engine.query(View::P1, "P1.leader.life").unwrap(),
            Some(json!(22_i64))
        );
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(18_i64))
        );
    }
}

#[test]
fn repeated_race_events_remain_distinct_across_restore_and_turns() {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([
        {"kind":"activated","line":1_i64,"body":{"op":"race","subjects":"self","count":2_i64}},
        {"kind":"trigger","line":2_i64,"event":"race","subject":"self","body":{"op":"modify","subjects":"self","power":1_i64}}
    ]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("race.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"e","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::clone(&catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "race",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
            "race-twice",
        )
        .unwrap();
    let choices = engine.legal().unwrap();
    assert_eq!(choices.len(), 2);
    assert_eq!(
        choices[0]["pending"]["event"],
        json!({"raced":"a","n":1_i64})
    );
    assert_eq!(
        choices[1]["pending"]["event"],
        json!({"raced":"a","n":2_i64})
    );
    engine.decide(&choices[1], "second-first").unwrap();
    let restored: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let rebuilt = Game::from_observation(
        catalog,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "race-world",
    )
    .unwrap();
    for mut candidate in [engine, restored, rebuilt] {
        assert_eq!(candidate.legal().unwrap(), vec![choices[0].clone()]);
        candidate.decide(&choices[0], "first-last").unwrap();
        assert_eq!(
            candidate.query(View::P1, "P1.field.a.power").unwrap(),
            Some(json!(4_i64))
        );
        for (decision, node) in [
            (json!({"do":"end-phase"}), "end-first"),
            (json!({"do":"pass"}), "start-second"),
            (json!({"do":"end-phase"}), "end-second"),
            (json!({"do":"pass"}), "start-first"),
        ] {
            candidate.decide(&decision, node).unwrap();
        }
        candidate
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                "race-again",
            )
            .unwrap();
        let later = candidate.legal().unwrap();
        assert_eq!(later[0]["pending"]["event"], json!({"raced":"a","n":3_i64}));
        assert_eq!(later[1]["pending"]["event"], json!({"raced":"a","n":4_i64}));
        assert_eq!(
            candidate
                .query(View::P1, "semantic_state.counters_this_turn.P1.evolutions")
                .unwrap(),
            Some(json!(0_i64))
        );
    }
}

#[test]
fn event_occurrence_history_is_validated_and_hidden_with_its_subject() {
    let catalog = Arc::new(catalog(&json!({"op":"draw","count":0_i64})));
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"secret","card":"unit-follower"}]);
    let entry = json!({"event":"race","subject":"secret","count":7_u64});
    initial["semantic_state"]["event_occurrences"] = json!([entry]);
    let engine = Game::new(
        Arc::clone(&catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "history",
    )
    .unwrap();
    assert_eq!(
        engine
            .query(View::P1, "semantic_state.event_occurrences")
            .unwrap(),
        Some(json!([]))
    );
    assert_eq!(
        engine
            .query(View::P2, "semantic_state.event_occurrences")
            .unwrap(),
        Some(json!([entry]))
    );
    for entries in [
        json!([entry, entry]),
        json!([{"event":"race","subject":"secret","count":0_i64}]),
        json!([{"event":"race","subject":"secret","count":-1_i64}]),
    ] {
        initial["semantic_state"]["event_occurrences"] = entries;
        Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "invalid",
        )
        .unwrap_err();
    }
}

#[test]
fn decline_alias_requires_a_legal_empty_selection() {
    for min in [0_i64, 1_i64] {
        let mut engine = game(&json!({"op":"seq","steps":[
            {"op":"select","select":{"side":"opponent","zone":"field"},"min":min,"max":1_i64,"bind":"chosen"},
            {"op":"damage","subjects":"chosen","amount":1_i64}
        ]}));
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        let before = engine.digest().unwrap();
        let legal = engine.legal().unwrap();
        assert_eq!(
            legal
                .iter()
                .any(|choice| *choice == json!({"do":"resolve-choice","select":[]})),
            min == 0
        );
        assert!(legal.iter().all(|choice| choice["choice"].is_null()));
        let step = engine
            .decide(
                &json!({"do":"resolve-choice","choice":"decline"}),
                "decline",
            )
            .unwrap();
        assert_eq!(
            step.outcome,
            if min == 0 { "resolved" } else { "cannot-play" }
        );
        assert_eq!(
            engine.query(View::P1, "P2.field.b.hp").unwrap(),
            Some(json!(3_i64))
        );
        if min == 1 {
            assert_eq!(engine.digest().unwrap(), before);
        }
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
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let sampled = Game::from_observation(catalog, &packet, "P1", "sample").unwrap();
    assert_eq!(sampled.legal().unwrap(), engine.legal().unwrap());
}

#[test]
fn imported_usage_tracks_object_generations_and_round_trips() {
    let mut doc = document(&json!({"op":"seq","steps":[
        {"op":"move","subjects":{"zone":"field","side":"self"},"to":"ex"},
        {"op":"move","subjects":{"zone":"ex","side":"self"},"to":"field"}
    ]}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{
        "kind":"activated","line":1_i64,"limit":1_i64,
        "body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}
    }]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("usage.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut position = setup();
    let entry = json!({"ability":{"source":"a","line":1_i64},"count":1_i64});
    position["semantic_state"] = json!({"used_this_turn":[entry]});
    let mut engine = Game::new(
        Arc::clone(&catalog),
        &position,
        &Value::Null,
        &Value::Null,
        "usage",
    )
    .unwrap();
    let activation = json!({"do":"activate","ability":{"source":"a","line":1_i64}});
    let before = engine.digest().unwrap();
    assert_eq!(
        engine.decide(&activation, "already-used").unwrap().outcome,
        "cannot-activate"
    );
    assert_eq!(engine.digest().unwrap(), before);
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "return",
        )
        .unwrap();
    assert!(engine.legal().unwrap().contains(&activation));
    engine.decide(&activation, "new-object").unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(19_i64))
    );
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(
        packet["semantic_state"]["used_this_turn"],
        json!([
            {"ability":{"source":"a","line":1_i64},"generation":2_i64,"count":1_i64}
        ])
    );
    let restored: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let sampled = Game::from_observation(Arc::clone(&catalog), &packet, "P1", "sample").unwrap();
    assert_eq!(restored.legal().unwrap(), engine.legal().unwrap());
    assert_eq!(sampled.legal().unwrap(), engine.legal().unwrap());
    assert!(
        sampled
            .legal()
            .unwrap()
            .iter()
            .all(|choice| choice["do"] != "activate")
    );
    position["semantic_state"]["used_this_turn"] = json!([entry, entry]);
    Game::new(
        Arc::clone(&catalog),
        &position,
        &Value::Null,
        &Value::Null,
        "duplicate",
    )
    .unwrap_err();
    position["semantic_state"]["used_this_turn"][0]["count"] = json!(-1_i64);
    Game::new(catalog, &position, &Value::Null, &Value::Null, "negative").unwrap_err();
}

#[test]
fn defeat_prohibition_preserves_life_triggers_and_expires_failed_draws() {
    let mut doc = document(&json!({"op":"seq","steps":[
        {"op":"damage","subjects":"self.leader","amount":2_i64},
        {"op":"draw","count":1_i64}
    ]}));
    doc["cards"]["unit-follower"]["abilities"] = json!([
        {"kind":"static","line":1_i64,"body":{"op":"restrict","subjects":{"zone":"leader","side":"self"},"action":"lose"}},
        {"kind":"trigger","line":2_i64,"event":"leader_life_change","subject":"self.leader",
         "trigger_if":{"fn":"and","args":[{"fn":"gt","args":[{"read":"event.before_life"},0_i64]},{"fn":"le","args":[{"read":"event.after_life"},0_i64]}]},
         "body":{"op":"seq","steps":[{"op":"move","subjects":"self","to":"cemetery"},{"op":"modify","subjects":"self.leader","set_hp":1_i64}]}}
    ]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("defeat.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut position = setup();
    position["players"]["P1"]["leader"]["life"] = json!(2_i64);
    position["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"s","card":"unit-spell"},{"id":"s2","card":"unit-spell"}
    ]);
    let mut engine = Game::new(catalog, &position, &Value::Null, &Value::Null, "defeat").unwrap();
    let first = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "protected",
        )
        .unwrap();
    assert_eq!(first.outcome, "resolved");
    assert!(first.events.iter().all(|event| event["kind"] != "敗北"));
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(
        packet["semantic_state"]["pending_triggers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let resolved = restored
        .decide(
            &json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":2_i64}}}),
            "recover",
        )
        .unwrap();
    assert_eq!(resolved.outcome, "resolved");
    assert_eq!(
        restored.query(View::P1, "P1.leader.life").unwrap(),
        Some(json!(1_i64))
    );
    assert_eq!(
        restored.query(View::P1, "P1.field").unwrap(),
        Some(json!([]))
    );
    assert_eq!(
        restored.query(View::P1, "game.ended").unwrap(),
        Some(json!(false))
    );
    let lost = restored
        .decide(
            &json!({"do":"play","card":"s2","targets":{"1":["b"]}}),
            "opponent-does-not-protect",
        )
        .unwrap();
    assert_eq!(lost.outcome, "game-end");
    assert!(
        lost.events
            .iter()
            .any(|event| event["kind"] == "敗北" && event["by"] == "rule-11.2.1")
    );
}

#[test]
fn effect_victory_stops_resolution_before_rule_defeat_unless_prohibited() {
    for prohibited in [false, true] {
        let mut doc = document(&json!({"op":"seq","steps":[
            {"op":"draw","count":1_i64},
            {"op":"win","side":"self"},
            {"op":"damage","subjects":"opponent.leader","amount":2_i64}
        ]}));
        if prohibited {
            doc["cards"]["unit-follower"]["abilities"] = json!([
                {"kind":"static","line":1_i64,"body":{"op":"restrict","subjects":"opponent.leader","action":"win"}}
            ]);
        }
        let catalog = Arc::new(
            Catalog::from_documents(
                &snapshot(),
                &registry(),
                &[("victory.yaml".into(), doc.to_string())],
            )
            .unwrap(),
        );
        let mut engine =
            Game::new(catalog, &setup(), &Value::Null, &Value::Null, "victory").unwrap();
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "victory",
            )
            .unwrap();
        assert_eq!(step.outcome, "game-end");
        assert_eq!(
            engine.query(View::P1, "game.winner").unwrap(),
            Some(json!(if prohibited { "P2" } else { "P1" }))
        );
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(if prohibited { 18_i64 } else { 20_i64 }))
        );
        assert_eq!(
            step.events.iter().any(|event| event["kind"] == "敗北"),
            prohibited
        );
        assert_eq!(
            step.events.iter().any(|event| event["kind"] == "解決"),
            prohibited
        );
        assert!(engine.legal().unwrap().is_empty());
    }
}

#[test]
fn end_discard_rechecks_after_triggers_and_roundtrips_private_choices() {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    doc["cards"]["unit-spell"]["abilities"] = json!([
        {"kind":"trigger","line":1_i64,"active_zones":["hand"],"event":"discard","subject":"self","body":{"op":"draw","count":1_i64}}
    ]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("end.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut position = setup();
    let mut hand = (0_u32..8)
        .map(|index| json!({"id":format!("h{index}"),"card":"unit-follower"}))
        .collect::<Vec<_>>();
    hand.push(json!({"id":"s","card":"unit-spell"}));
    position["players"]["P1"]["zones"]["hand"] = json!(hand);
    position["players"]["P1"]["zones"]["deck"] = json!([{"id":"d1","card":"unit-follower"}]);
    position["players"]["P2"]["zones"]["deck"] = json!([{"id":"d2","card":"unit-follower"}]);
    let mut engine = Game::new(catalog, &position, &Value::Null, &Value::Null, "end").unwrap();
    engine.decide(&json!({"do":"end-phase"}), "end").unwrap();
    engine.decide(&json!({"do":"pass"}), "quick-pass").unwrap();
    assert_eq!(engine.legal().unwrap().len(), 36);
    assert_eq!(
        engine.projection(View::P2).unwrap()["awaiting"],
        json!({"by":"P1"})
    );
    let before = engine.digest().unwrap();
    for selection in [json!(["s", "s"]), json!(["s"]), json!(["s", "b"])] {
        assert_eq!(
            engine
                .decide(&json!({"do":"end-discard","select":selection}), "invalid")
                .unwrap()
                .outcome,
            "cannot-play"
        );
        assert_eq!(engine.digest().unwrap(), before);
    }
    let discarded = engine
        .decide(&json!({"do":"end-discard","select":["s","h7"]}), "discard")
        .unwrap();
    assert_eq!(discarded.outcome, "resolved");
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    restored
        .decide(
            &json!({"do":"choose-pending","pending":{"ability":{"source":"s","line":1_i64}}}),
            "draw",
        )
        .unwrap();
    assert_eq!(restored.legal().unwrap().len(), 8);
    assert_eq!(
        restored.query(View::P1, "turn.active").unwrap(),
        Some(json!("P1"))
    );
    restored
        .decide(
            &json!({"do":"end-discard","select":["d1"]}),
            "discard-again",
        )
        .unwrap();
    assert_eq!(
        restored.query(View::P1, "turn.active").unwrap(),
        Some(json!("P2"))
    );
    assert_eq!(
        restored.query(View::P2, "P2.hand").unwrap(),
        Some(json!(["d2"]))
    );
    assert_eq!(
        restored.query(View::P1, "P1.hand_count").unwrap(),
        Some(json!(7_i64))
    );
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
fn printed_line_discriminators_select_the_intended_pending_ability() {
    for shared_line in [false, true] {
        let second_line = if shared_line { 1_i64 } else { 2_i64 };
        let mut doc = document(&json!({"op":"draw","count":0_i64}));
        doc["cards"]["unit-follower"]["abilities"] = json!([
            {"kind":"trigger","line":1_i64,"keyword":"first","event":"attack","subject":"self",
             "body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}},
            {"kind":"trigger","line":second_line,"keyword":"second","event":"attack","subject":"self",
             "body":{"op":"damage","subjects":"opponent.leader","amount":2_i64}}
        ]);
        let words = json!({"version":"astra/1","keywords":{
            "first":{"ja":"First","role":"ability-label"},
            "second":{"ja":"Second","role":"ability-label"}
        }});
        let catalog = Arc::new(
            Catalog::from_documents(
                &snapshot(),
                &words.to_string(),
                &[("discriminators.yaml".into(), doc.to_string())],
            )
            .unwrap(),
        );
        let mut engine = Game::new(
            catalog,
            &setup(),
            &Value::Null,
            &Value::Null,
            "discriminators",
        )
        .unwrap();
        engine
            .decide(
                &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
                "attack",
            )
            .unwrap();
        let choices = engine.legal().unwrap();
        assert_eq!(choices.len(), 2);
        for (choice, (line, label)) in choices
            .iter()
            .zip([(1_i64, "First"), (second_line, "Second")])
        {
            let mut reference = json!({"source":"a","line":line});
            if shared_line {
                reference["keyword"] = json!(label);
            }
            assert_eq!(
                *choice,
                json!({"do":"choose-pending","pending":{"ability":reference}})
            );
        }
        if shared_line {
            let before = engine.digest().unwrap();
            engine.decide(&json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1_i64}}}), "ambiguous").unwrap_err();
            assert_eq!(engine.digest().unwrap(), before);
        }
        engine.decide(&choices[1], "second").unwrap();
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(18_i64))
        );
        assert_eq!(engine.legal().unwrap(), vec![choices[0].clone()]);
        engine.decide(&choices[0], "first").unwrap();
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(17_i64))
        );
    }
}

#[test]
fn pending_events_preserve_instance_identity_until_resolution() {
    let mut doc = document(&json!({"op":"destroy","subjects":"target.1"}));
    doc["cards"]["unit-spell"]["abilities"][0]["targets"][0]["min"] = json!(2_i64);
    doc["cards"]["unit-spell"]["abilities"][0]["targets"][0]["max"] = json!(2_i64);
    doc["cards"]["unit-follower"]["abilities"] = json!([{
        "kind":"trigger","line":1_i64,"event":"leave",
        "subject":{"zone":"field","side":"opponent","type":"follower"},
        "body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}
    }]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("repeated.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"b","card":"unit-follower"}, {"id":"c","card":"unit-follower"}
    ]);
    let mut engine = Game::new(
        Arc::clone(&catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "repeated",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b","c"]}}),
            "cast",
        )
        .unwrap();
    let choices = engine.legal().unwrap();
    assert_eq!(choices.len(), 2);
    for (choice, object) in choices.iter().zip(["b", "c"]) {
        assert_eq!(
            *choice,
            json!({"do":"choose-pending","pending":{
                "ability":{"source":"a","line":1_i64},"event":{"left_field":object}
            }})
        );
    }
    assert_eq!(
        engine.projection(View::P1).unwrap()["awaiting"]["choices"],
        json!(choices)
    );
    let ambiguous =
        json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1_i64}}});
    let before = engine.digest().unwrap();
    engine.decide(&ambiguous, "ambiguous").unwrap_err();
    assert_eq!(engine.digest().unwrap(), before);
    engine.decide(&choices[1], "second-instance").unwrap();
    let singleton = &choices[0];
    assert_eq!(engine.legal().unwrap(), vec![singleton.clone()]);
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(packet["awaiting"]["choices"], json!([singleton]));
    assert_eq!(
        packet["semantic_state"]["pending_triggers"][0]["event"],
        json!({"left_field":"b"})
    );
    let mut sampled = Game::from_observation(catalog, &packet, "P1", "pending-sample").unwrap();
    assert_eq!(sampled.legal().unwrap(), engine.legal().unwrap());
    sampled.decide(singleton, "sampled-instance").unwrap();
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    restored.decide(singleton, "last-instance").unwrap();
    assert_eq!(
        restored.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(18_i64))
    );
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
    assert_eq!(decision["pending"]["ability"]["delayed"], true);
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
    let mut engine = game(&json!({"op":"modify","subjects":"target.1","traits":["unmodeled"]}));
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

#[test]
fn semantic_digest_preserves_selector_sources_in_suspended_programs() {
    let mut engine = game(&json!({"op":"seq","steps":[
        {"op":"optional","then":{"op":"draw","count":0_i64}},
        {"op":"damage","subjects":{"from":"target.1"},"amount":1_i64}
    ]}));
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let mut state = serde_json::to_value(&engine).unwrap();
    assert_eq!(
        state["state"]["frame"]["todo"][0]["subjects"]["from"],
        "target.1"
    );
    state["state"]["frame"]["todo"][0]["subjects"]["from"] = json!("self");
    let changed: Game = serde_json::from_value(state).unwrap();
    assert_ne!(changed.digest().unwrap(), engine.digest().unwrap());
}

#[test]
fn identical_pending_copies_preserve_multiplicity_after_restore() {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{
        "kind":"trigger","line":1_i64,"event":"end",
        "body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}
    }]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("copies.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut position = setup();
    let pending =
        json!({"controller":"P1","ability":{"source":"a","line":1_i64},"event":{"end":"P1"}});
    position["semantic_state"]["pending_triggers"] = json!([pending, pending, pending]);
    let engine = Game::new(catalog, &position, &Value::Null, &Value::Null, "copies").unwrap();
    let choices = engine.legal().unwrap();
    assert_eq!(choices.len(), 3);
    assert!(choices.iter().all(|choice| *choice == choices[0]));
    assert!(choices[0]["pending"].get("event").is_none());
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    for remaining in (0..3).rev() {
        restored.decide(&choices[0], "copy").unwrap();
        assert_eq!(
            restored
                .legal()
                .unwrap()
                .iter()
                .filter(|choice| choice["do"] == "choose-pending")
                .count(),
            remaining
        );
    }
    assert_eq!(
        restored.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(17_i64))
    );
}

#[test]
fn partial_shuffle_preserves_other_cards_and_rejects_full_deck_scripts() {
    let body = json!({"op":"shuffle","subjects":{"zone":"deck","side":"self","where":{"fn":"ge","args":[{"read":"item.power"},7_i64]}}});
    let catalog = Arc::new(catalog(&body));
    let mut position = setup();
    position["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"unit-follower"},
        {"id":"d2","card":"unit-follower","state":{"power":7_i64}},
        {"id":"d3","card":"unit-follower"},
        {"id":"d4","card":"unit-follower","state":{"power":8_i64}}
    ]);
    for (result, accepted) in [
        (json!(["d4", "d2"]), true),
        (json!(["d1", "d4", "d3", "d2"]), false),
        (json!(["d2", "d2"]), false),
    ] {
        let mut engine = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &json!({"shuffles":[{"player":"P1","result":result}]}),
            "subset",
        )
        .unwrap();
        let before = engine.digest().unwrap();
        let step = engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "shuffle",
        );
        if accepted {
            step.unwrap();
            assert_eq!(
                engine.query(View::Referee, "P1.deck").unwrap(),
                Some(json!(["d1", "d4", "d3", "d2"]))
            );
        } else {
            step.unwrap_err();
            assert_eq!(engine.digest().unwrap(), before);
        }
    }
}

#[test]
fn constrained_selection_order_and_open_integer_declaration_restore_together() {
    let body = json!({"op":"seq","steps":[
        {"op":"look","count":3_i64,"bind":"looked"},
        {"op":"select","select":"looked","min":0_i64,"max":3_i64,"bind":"chosen","constraint":{"fn":"le","args":[{"fn":"sum","args":[{"values":"chosen","field":"cost"}]},1_i64]}},
        {"op":"select","select":{"difference":["looked","chosen"]},"min":2_i64,"max":2_i64,"bind":"ordered","order":true},
        {"op":"declare_number","bind":"number"},
        {"op":"move","subjects":"ordered","to":"deck","position":"bottom"},
        {"op":"damage","subjects":"opponent.leader","amount":{"read":"number"}}
    ]});
    let mut position = setup();
    position["players"]["P2"]["leader"]["life"] = json!(100_i64);
    position["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"unit-follower"},{"id":"d2","card":"unit-follower"},{"id":"d3","card":"unit-follower"}
    ]);
    let mut engine = Game::new(
        Arc::new(catalog(&body)),
        &position,
        &Value::Null,
        &Value::Null,
        "constraints",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "start",
        )
        .unwrap();
    let before = engine.digest().unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"resolve-choice","select":["d1","d2"]}),
                "too-many"
            )
            .unwrap()
            .outcome,
        "cannot-play"
    );
    assert_eq!(engine.digest().unwrap(), before);
    engine
        .decide(&json!({"do":"resolve-choice","select":["d1"]}), "subset")
        .unwrap();
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    assert_eq!(restored.legal().unwrap().len(), 2);
    restored
        .decide(&json!({"do":"resolve-choice","order":["d3","d2"]}), "order")
        .unwrap();
    let before_declaration = restored.digest().unwrap();
    assert_eq!(
        restored
            .decide(&json!({"do":"resolve-choice","declare":"37"}), "wrong-type")
            .unwrap()
            .outcome,
        "cannot-play"
    );
    assert_eq!(restored.digest().unwrap(), before_declaration);
    restored
        .decide(&json!({"do":"resolve-choice","declare":37_i64}), "integer")
        .unwrap();
    assert_eq!(
        restored.query(View::Referee, "P1.deck").unwrap(),
        Some(json!(["d1", "d3", "d2"]))
    );
    assert_eq!(
        restored.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(63_i64))
    );
}

#[test]
fn trigger_context_keeps_last_known_values_after_leaving_and_serializing() {
    let mut doc = document(&json!({"op":"move","subjects":"target.1","to":"cemetery"}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{
        "line":1_i64,"kind":"trigger","event":"leave","subject":"self",
        "body":{"op":"damage","subjects":"opponent.leader","amount":{"read":"event.subject.power"}}
    }]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("context.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut position = setup();
    position["players"]["P2"]["zones"]["field"][0]["state"]["power"] = json!(8_i64);
    let mut engine =
        Game::new(catalog, &position, &Value::Null, &Value::Null, "last-info").unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "leave",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.cemetery.b.power").unwrap(),
        Some(json!(2_i64))
    );
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let decision = restored.legal().unwrap().remove(0);
    restored.decide(&decision, "resolve").unwrap();
    assert_eq!(
        restored.query(View::P1, "P1.leader.life").unwrap(),
        Some(json!(12_i64))
    );
}

#[test]
fn movement_prohibition_beats_replacement_and_delayed_events_use_actual_destination() {
    for prohibited in [false, true] {
        let mut doc = document(&json!({"op":"seq","steps":[
            {"op":"delay","event":"field_to_cemetery","subjects":"target.1","once":true,"until":"end-of-turn","body":{"op":"damage","subjects":"opponent.leader","amount":4_i64}},
            {"op":"move","subjects":"target.1","to":"cemetery"}
        ]}));
        doc["cards"]["unit-follower"]["abilities"] = json!([
            {"line":1_i64,"kind":"static","body":{"op":"replace_move","subjects":{"zone":"field","side":"opponent"},"from":"field","to":"cemetery","replacement":"banish"}},
            {"line":2_i64,"kind":"static","body":{"op":"restrict","subjects":"self","action":"banish","condition":prohibited}}
        ]);
        let catalog = Arc::new(
            Catalog::from_documents(
                &snapshot(),
                &registry(),
                &[("replace.yaml".into(), doc.to_string())],
            )
            .unwrap(),
        );
        let mut engine =
            Game::new(catalog, &setup(), &Value::Null, &Value::Null, "movement").unwrap();
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        let destination = if prohibited { "cemetery" } else { "banish" };
        assert_eq!(
            engine
                .query(View::Referee, &format!("P2.{destination}"))
                .unwrap(),
            Some(json!(["b"]))
        );
        let pending = engine
            .legal()
            .unwrap()
            .into_iter()
            .filter(|choice| choice["do"] == "choose-pending")
            .collect::<Vec<_>>();
        assert_eq!(pending.len(), usize::from(prohibited));
        if prohibited {
            assert_eq!(pending[0]["pending"]["ability"]["delayed"], true);
            let mut restored: Game =
                serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
            restored.decide(&pending[0], "delayed").unwrap();
            assert_eq!(
                restored.query(View::P1, "P2.leader.life").unwrap(),
                Some(json!(16_i64))
            );
            assert!(
                restored
                    .legal()
                    .unwrap()
                    .iter()
                    .all(|choice| choice["do"] != "choose-pending")
            );
        }
    }
}

#[test]
fn nested_play_preserves_outer_continuation_across_inner_target_and_declaration() {
    let mut doc = document(&json!({"op":"damage","subjects":"target.1","amount":1_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([
        {"line":1_i64,"kind":"activated","body":{"op":"seq","steps":[{"op":"play_ability","subjects":"self","event":"evolve"},{"op":"modify","subjects":"self.leader","hp":2_i64}]}},
        {"line":2_i64,"kind":"trigger","event":"evolve","subject":"self","targets":[{"key":"1","select":{"side":"opponent","zone":"field"},"min":1_i64,"max":1_i64}],"costs":[{"op":"pp","amount":1_i64}],"body":{"op":"seq","steps":[{"op":"declare_number","bind":"declared"},{"op":"damage","subjects":"target.1","amount":{"read":"declared"}}]}}
    ]);
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("nested.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut engine = Game::new(catalog, &setup(), &Value::Null, &Value::Null, "nested").unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                "outer"
            )
            .unwrap()
            .outcome,
        "paused"
    );
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let choose = json!({"do":"resolve-choice","targets":{"1":["b"]}});
    assert_eq!(
        restored.decide(&choose, "target").unwrap().outcome,
        "paused"
    );
    let mut resumed: Game =
        serde_json::from_str(&serde_json::to_string(&restored).unwrap()).unwrap();
    assert_eq!(
        resumed
            .decide(&json!({"do":"resolve-choice","declare":2_i64}), "number")
            .unwrap()
            .outcome,
        "resolved"
    );
    assert_eq!(
        resumed.query(View::Referee, "P2.field.b.hp").unwrap(),
        Some(json!(1_i64))
    );
    assert_eq!(
        resumed.query(View::Referee, "P1.leader.life").unwrap(),
        Some(json!(22_i64))
    );
    assert_eq!(
        resumed.query(View::Referee, "P1.pp.current").unwrap(),
        Some(json!(1_i64))
    );
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "The opening fixture requires a registered synthetic start amulet."
)]
fn opening_catalog() -> Arc<Catalog> {
    let mut doc = document(&json!({"op":"seq","steps":[]}));
    doc["cards"]["unit-follower"]["abilities"] =
        json!([{ "kind":"static","line":1_i64,"body":{"op":"keyword","name":"start_amulet"}}]);
    let mut keywords: Value = serde_json::from_str(&registry()).unwrap();
    keywords["keywords"]["start_amulet"] = json!({"ja":"スタートアミュレット","rule":"14.4.3","expansion":{"op":"keyword","name":"start_amulet"}});
    Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &keywords.to_string(),
            &[("opening.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    )
}

#[test]
fn pregame_facedown_choice_is_private_and_roundtrips_before_mulligan() {
    let catalog = opening_catalog();
    let position = json!({"pregame":true,"players":{
        "P1":{"construction":"title","title":"カードファイト!! ヴァンガード","leader":{"class":"ニュートラル"},"deck_list":[{"id":"amulet","card":"unit-follower"},{"filler":40_i64}],"evolve_deck_list":[]},
        "P2":{"construction":"class","leader":{"class":"ニュートラル"},"deck_list":[{"filler":40_i64}],"evolve_deck_list":[]}
    }});
    let random = json!({"first_chooser":"P2","shuffles":[{"player":"P1","zone":"deck","result":[{"filler":40_i64}]},{"player":"P2","zone":"deck","result":[{"filler":40_i64}]}]});
    let mut engine = Game::new(catalog, &position, &Value::Null, &random, "opening").unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"choose-start-amulet","object":"amulet"}),
                "choose"
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    let private = engine.projection(View::P2).unwrap();
    assert!(!private.to_string().contains("\"amulet\""));
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let first = restored
        .decide(&json!({"do":"choose-first","first":"P1"}), "first")
        .unwrap();
    assert!(first.events.iter().all(|event| event["kind"] != "引く"));
    restored
        .decide(&json!({"do":"mulligan","redo":false}), "keep-first")
        .unwrap();
    restored
        .decide(&json!({"do":"mulligan","redo":false}), "keep-second")
        .unwrap();
    assert_eq!(
        restored.query(View::P2, "P1.field.amulet.face_up").unwrap(),
        Some(json!(true))
    );
    assert_eq!(
        restored.query(View::P2, "P2.ep").unwrap(),
        Some(json!(3_i64))
    );
    assert_eq!(
        restored.query(View::P1, "P1.pp.current").unwrap(),
        Some(json!(1_i64))
    );
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "Synthetic fixture maps are constructed here; malformed fixtures must fail the test."
)]
fn stack_catalog(body: &Value) -> Catalog {
    let soil = json!({"number":"unit-soil","faces":[{"name":"大地の魔片","card_class":"ウィッチ","card_type":"アミュレット・トークン","cost":"1","traits":[],"text":null,"sections":[]}]});
    let mut docs = document(body);
    docs["cards"]["unit-soil"] = json!({"status":"complete","review":"synthetic","abilities":[{"kind":"static","line":1_i64,"body":{"op":"keyword","name":"stack"}}]});
    let mut keywords: Value = serde_json::from_str(&registry()).unwrap();
    keywords["keywords"]["stack"] =
        json!({"ja":"スタック","rule":"13.3.2","expansion":{"op":"keyword","name":"stack"}});
    keywords["keywords"]["stack_counter"] =
        json!({"ja":"スタックカウンター","expansion":{"op":"keyword","name":"stack_counter"}});
    Catalog::from_documents(
        &format!("{}\n{soil}", snapshot()),
        &keywords.to_string(),
        &[("stack.yaml".into(), docs.to_string())],
    )
    .unwrap()
}

#[test]
fn stack_enters_with_counters_and_waits_for_recipient_even_when_unique() {
    for existing in [false, true] {
        let catalog = Arc::new(stack_catalog(&json!({"op":"seq","steps":[
            {"op":"create","name":"大地の魔片","count":1_i64,"to":"field"},
            {"op":"stack","amount":2_i64},
            {"op":"damage","subjects":"target.1","amount":1_i64}
        ]})));
        let mut initial = setup();
        if existing {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"a","card":"unit-soil","state":{"counters":{"スタックカウンター":4_i64}}}]);
        }
        let mut engine = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "stack",
        )
        .unwrap();
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        assert_eq!(step.outcome, "paused");
        assert_eq!(
            engine
                .query(View::P1, "P1.field.new-1.counters.スタックカウンター")
                .unwrap(),
            Some(json!(1_i64))
        );
        assert_eq!(
            engine.query(View::P1, "P2.field.b.hp").unwrap(),
            Some(json!(3_i64))
        );
        assert!(
            !step
                .events
                .iter()
                .any(|event| event["kind"] == "カウンター")
        );
        let legal = engine.legal().unwrap();
        assert_eq!(legal.len(), if existing { 2 } else { 1 });
        let encoded = serde_json::to_string(&engine).unwrap();
        let mut restored: Game = serde_json::from_str(&encoded).unwrap();
        let observation = engine.projection(View::P1).unwrap();
        let mut sampled = Game::from_observation(catalog, &observation, "P1", "restore").unwrap();
        for instance in [&mut restored, &mut sampled] {
            assert_eq!(instance.legal().unwrap(), legal);
            let done = instance
                .decide(&json!({"do":"resolve-choice","select":["new-1"]}), "select")
                .unwrap();
            assert_eq!(done.outcome, "resolved");
            assert_eq!(
                instance
                    .query(View::P1, "P1.field.new-1.counters.スタックカウンター")
                    .unwrap(),
                Some(json!(3_i64))
            );
            assert_eq!(
                instance.query(View::P1, "P2.field.b.hp").unwrap(),
                Some(json!(2_i64))
            );
            if existing {
                assert_eq!(
                    instance
                        .query(View::P1, "P1.field.a.counters.スタックカウンター")
                        .unwrap(),
                    Some(json!(4_i64))
                );
            }
        }
    }
}

#[test]
fn stack_without_recipient_replaces_entry_count_and_obeys_capacity() {
    for (amount, full) in [(3_i64, false), (0, false), (2, true)] {
        let catalog = Arc::new(stack_catalog(&json!({"op":"stack","amount":amount})));
        let mut initial = setup();
        if full {
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0_u32..5)
                    .map(|n| json!({"id":format!("a{n}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            );
        }
        let mut engine = Game::new(catalog, &initial, &Value::Null, &Value::Null, "stack").unwrap();
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        assert_eq!(step.outcome, if full { "paused" } else { "resolved" });
        if full {
            let legal = engine.legal().unwrap();
            assert_eq!(legal.len(), 1);
            assert_eq!(legal[0]["select"], json!([]));
            engine
                .decide(&json!({"do":"resolve-choice","select":[]}), "no-space")
                .unwrap();
            assert_eq!(
                engine.query(View::P1, "P1.field_count").unwrap(),
                Some(json!(5_i64))
            );
        } else if amount == 0 {
            assert_eq!(
                engine.query(View::P1, "P1.field_count").unwrap(),
                Some(json!(1_i64))
            );
            assert!(!step.events.iter().any(|event| event["kind"] == "場に出す"));
        } else {
            assert_eq!(
                engine
                    .query(View::P1, "P1.field.new-1.counters.スタックカウンター")
                    .unwrap(),
                Some(json!(amount))
            );
        }
    }
}

#[test]
fn labeled_keyword_choices_preserve_modes_and_survive_restore() {
    let body = json!({"op":"seq","steps":[
        {"op":"choice","timing":"resolve","min":1_i64,"max":1_i64,
         "labels":[{"keyword":"guard"},{"keyword":"drain"}],
         "modes":[{"op":"modify","subjects":"target.1","keywords":["guard"]},{"op":"modify","subjects":"target.1","keywords":["drain"]}]},
        {"op":"modify","subjects":"self.leader","hp":-1_i64}
    ]});
    let catalog = Arc::new(catalog(&body));
    let mut engine = Game::new(
        Arc::clone(&catalog),
        &setup(),
        &Value::Null,
        &Value::Null,
        "labels",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.legal().unwrap(),
        json!([
            {"do":"resolve-choice","keyword":"守護"},{"do":"resolve-choice","keyword":"ドレイン"}
        ])
        .as_array()
        .unwrap()
        .clone()
    );
    let before = engine.digest().unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"resolve-choice","options":[2_i64]}),
                "wrong-label"
            )
            .unwrap()
            .outcome,
        "cannot-play"
    );
    assert_eq!(engine.digest().unwrap(), before);
    let bytes = serde_json::to_string(&engine).unwrap();
    let mut restored: Game = serde_json::from_str(&bytes).unwrap();
    let mut sampled = Game::from_observation(
        catalog,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "labels",
    )
    .unwrap();
    for instance in [&mut restored, &mut sampled] {
        assert_eq!(
            instance
                .decide(
                    &json!({"do":"resolve-choice","keyword":"ドレイン"}),
                    "choose"
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        assert_eq!(
            instance.query(View::P1, "P2.field.b.keywords").unwrap(),
            Some(json!(["ドレイン"]))
        );
        assert_eq!(
            instance.query(View::P1, "P1.leader.life").unwrap(),
            Some(json!(19_i64))
        );
    }
}

#[test]
fn labeled_positions_execute_their_declared_branch_and_invalid_labels_fail_loading() {
    let body = json!({"op":"choice","timing":"resolve","min":1_i64,"max":1_i64,
        "labels":[{"position":"top"},{"position":"bottom"}],
        "modes":[{"op":"move","subjects":"target.1","to":"deck","position":"top"},{"op":"move","subjects":"target.1","to":"deck","position":"bottom"}]});
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"c","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::new(catalog(&body)),
        &initial,
        &Value::Null,
        &Value::Null,
        "position",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    engine
        .decide(
            &json!({"do":"resolve-choice","position":"bottom"}),
            "bottom",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::Referee, "P2.deck").unwrap(),
        Some(json!(["c", "b"]))
    );
    for change in [
        json!({"labels":[{"position":"top"}]}),
        json!({"labels":[{"position":"top"},{"position":"top"}]}),
        json!({"labels":[{"keyword":"unregistered"},{"keyword":"drain"}]}),
        json!({"labels":[{"position":"middle"},{"position":"bottom"}]}),
        json!({"min":0_i64}),
        json!({"max":2_i64}),
        json!({"timing":"play"}),
    ] {
        let mut candidate = body.clone();
        for (key, value) in change.as_object().unwrap() {
            candidate[key] = value.clone();
        }
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("bad.yaml".into(), document(&candidate).to_string())],
        )
        .unwrap_err();
    }
    let mut registry: Value = serde_json::from_str(&registry()).unwrap();
    registry["keywords"]["drain"]["ja"] = json!("守護");
    let mut candidate = body;
    candidate["labels"] = json!([{"keyword":"guard"},{"keyword":"drain"}]);
    Catalog::from_documents(
        &snapshot(),
        &registry.to_string(),
        &[("duplicate.yaml".into(), document(&candidate).to_string())],
    )
    .unwrap_err();
}

#[test]
fn bane_rule_destruction_respects_immunity_and_shares_the_lethal_damage_batch() {
    for (protected, power) in [(true, 0_i64), (true, 2), (true, 3), (false, 0)] {
        let mut docs =
            document(&json!({"op":"modify","subjects":"target.1","remove_abilities":true}));
        docs["cards"]["unit-follower"]["abilities"] =
            json!([{"kind":"static","line":1_i64,"body":{"op":"keyword","name":"bane"}}]);
        docs["cards"]["unit-tank"] = json!({"status":"complete","review":"synthetic","abilities":if protected {json!([{"kind":"static","line":1_i64,"body":{"op":"restrict","subjects":"self","action":"ability_destroy"}}])}else{json!([])}});
        let tank = json!({"number":"unit-tank","faces":[{"name":"tank","card_type":"フォロワー","card_class":"ニュートラル","traits":[],"cost":"1","power":"4","hp":"3","text":null,"sections":[]}]});
        let registry = json!({"version":"astra/1","keywords":{"bane":{"ja":"必殺","expansion":{"op":"keyword","name":"bane"}}}});
        let loaded = Catalog::from_documents(
            &format!("{}\n{tank}", snapshot()),
            &registry.to_string(),
            &[("bane.yaml".into(), docs.to_string())],
        )
        .unwrap();
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"power":power});
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"b","card":"unit-tank","state":{"acted":true}}]);
        let mut engine = Game::new(
            Arc::new(loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "bane",
        )
        .unwrap();
        engine
            .decide(
                &json!({"do":"attack","attacker":"a","target":"b"}),
                "battle",
            )
            .unwrap();
        let step = engine.decide(&json!({"do":"pass"}), "quick").unwrap();
        let destroyed = step
            .events
            .iter()
            .filter(|event| event["kind"] == "破壊")
            .collect::<Vec<_>>();
        assert_eq!(destroyed[0]["object"], json!("a"));
        assert_eq!(destroyed[0]["by"], json!("rule-11.3.1"));
        if protected && power < 3 {
            assert_eq!(destroyed.len(), 1);
            assert_eq!(
                engine.query(View::P1, "P2.field").unwrap(),
                Some(json!(["b"]))
            );
            engine
                .decide(
                    &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                    "remove-protection",
                )
                .unwrap();
            assert_eq!(
                engine.query(View::P1, "P2.field").unwrap(),
                Some(json!(["b"]))
            );
        } else {
            assert_eq!(destroyed.len(), 2);
            assert_eq!(destroyed[0]["group"], destroyed[1]["group"]);
            let reason = if power < 3 {
                "rule-11.3.2"
            } else {
                "rule-11.3.1"
            };
            assert_eq!(destroyed[1]["by"], json!(reason));
            assert!(step.events.iter().any(|event| event["kind"] == "移動"
                && event["object"] == "b"
                && event["by"] == reason));
            assert_eq!(
                engine.query(View::P1, "P2.cemetery").unwrap(),
                Some(json!(["b"]))
            );
        }
    }
}

#[test]
fn damage_caps_read_the_incoming_amount_and_reject_unresolved_dynamic_overlap() {
    for (amount, overlap) in [
        (0_i64, false),
        (3, false),
        (4, false),
        (9, false),
        (4, true),
    ] {
        let mut docs = document(&json!({"op":"damage","subjects":"target.1","amount":amount}));
        let mut replacements = vec![
            json!({"kind":"static","line":1_i64,"body":{"op":"replace_damage","subjects":"self","kind":"any","set":3_i64,"condition":{"fn":"ge","args":[{"read":"damage.amount"},4_i64]}}}),
        ];
        if overlap {
            replacements.push(json!({"kind":"static","line":2_i64,"body":{"op":"replace_damage","subjects":"self","kind":"any","amount":-1_i64}}));
        }
        docs["cards"]["unit-follower"]["abilities"] = json!(replacements);
        let loaded = Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("cap.yaml".into(), docs.to_string())],
        )
        .unwrap();
        let mut initial = setup();
        initial["players"]["P2"]["zones"]["field"][0]["state"] =
            json!({"hp":20_i64,"max_hp":20_i64});
        let mut engine = Game::new(
            Arc::new(loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "cap",
        )
        .unwrap();
        let before = engine.digest().unwrap();
        let outcome = engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        );
        if overlap {
            assert!(matches!(
                outcome.unwrap_err(),
                EngineFailure::Unsupported(_)
            ));
            assert_eq!(engine.digest().unwrap(), before);
        } else {
            let step = outcome.unwrap();
            assert_eq!(
                engine.query(View::P1, "P2.field.b.hp").unwrap(),
                Some(json!(20_i64.saturating_sub(amount.min(3))))
            );
            assert_eq!(
                step.events
                    .iter()
                    .filter(|event| event["kind"] == "ダメージ")
                    .count(),
                usize::from(amount > 0)
            );
        }
    }
}

#[test]
fn temporary_damage_prevention_survives_its_source_and_expires_at_turn_end() {
    let body = json!({"op":"seq","steps":[
        {"op":"replace_damage","subjects":"target.1","prevent":true,"kind":"any","until":"end-of-turn"},
        {"op":"damage","subjects":"target.1","amount":2_i64}
    ]});
    let loaded = Arc::new(catalog(&body));
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"c","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "prevent",
    )
    .unwrap();
    let cast = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert!(!cast.events.iter().any(|event| event["kind"] == "ダメージ"));
    let mut restored = Game::from_observation(
        loaded,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "restore",
    )
    .unwrap();
    for instance in [&mut engine, &mut restored] {
        instance
            .decide(
                &json!({"do":"attack","attacker":"a","target":"b"}),
                "battle",
            )
            .unwrap();
        instance.decide(&json!({"do":"pass"}), "quick").unwrap();
        assert_eq!(
            instance.query(View::P1, "P2.field.b.hp").unwrap(),
            Some(json!(3_i64))
        );
        instance.decide(&json!({"do":"end-phase"}), "end").unwrap();
        instance.decide(&json!({"do":"pass"}), "end-quick").unwrap();
        instance
            .decide(
                &json!({"do":"attack","attacker":"b","target":"a"}),
                "next-battle",
            )
            .unwrap();
        instance
            .decide(&json!({"do":"pass"}), "next-quick")
            .unwrap();
        assert_eq!(
            instance.query(View::P1, "P2.field.b.hp").unwrap(),
            Some(json!(1_i64))
        );
    }
}

#[test]
fn damage_prevention_tracks_the_recipient_generation_and_attack_kind() {
    let mut engine = game(&json!({"op":"seq","steps":[
        {"op":"replace_damage","subjects":"target.1","prevent":true,"kind":"any","until":"end-of-turn"},
        {"op":"move","subjects":"target.1","to":"hand"},
        {"op":"move","subjects":"b","to":"field"},
        {"op":"damage","subjects":"b","amount":2_i64}
    ]}));
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.field.b.hp").unwrap(),
        Some(json!(1_i64))
    );
    let docs =
        document(&json!({"op":"replace_damage","subjects":"self","prevent":true,"kind":"attack"}));
    let mut docs = docs;
    docs["cards"]["unit-follower"]["abilities"] = json!([{"kind":"static","line":1_i64,"body":{"op":"replace_damage","subjects":"self","prevent":true,"kind":"attack"}}]);
    let loaded = Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("attack.yaml".into(), docs.to_string())],
    )
    .unwrap();
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut combat = Game::new(
        Arc::new(loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "attack",
    )
    .unwrap();
    combat
        .decide(
            &json!({"do":"attack","attacker":"a","target":"b"}),
            "battle",
        )
        .unwrap();
    combat.decide(&json!({"do":"pass"}), "quick").unwrap();
    assert_eq!(
        combat.query(View::P1, "P1.field.a.hp").unwrap(),
        Some(json!(1_i64))
    );
    assert_eq!(
        combat.query(View::P1, "P2.field.b.hp").unwrap(),
        Some(json!(3_i64))
    );
}

#[test]
fn related_player_counts_distinguish_controller_owner_and_hidden_cardinality() {
    let body = json!({"op":"seq","steps":[
        {"op":"control","subjects":"target.1","side":"self"},
        {"op":"damage","subjects":"opponent.leader","amount":{"read":"target.1.controller.field_count"}},
        {"op":"damage","subjects":"opponent.leader","amount":{"read":"target.1.owner.field_count"}},
        {"op":"damage","subjects":"opponent.leader","amount":{"read":"target.1.owner.hand_count"}}
    ]});
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["hand"] = json!([{"filler":4_i64}]);
    let mut engine = Game::new(
        Arc::new(catalog(&body)),
        &initial,
        &Value::Null,
        &Value::Null,
        "player-count",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(14_i64))
    );
    assert_eq!(
        engine.query(View::P1, "P1.field").unwrap(),
        Some(json!(["a", "b"]))
    );
    for path in [
        "target.1.controller.field_count",
        "target.1.controller.unknown_count",
    ] {
        let mut rejected = game(&json!({"op":"seq","steps":[
            {"op":"move","subjects":"target.1","to":"hand"},
            {"op":"damage","subjects":"opponent.leader","amount":{"read":path}}
        ]}));
        let before = rejected.digest().unwrap();
        assert!(matches!(
            rejected
                .decide(
                    &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                    "unsupported"
                )
                .unwrap_err(),
            EngineFailure::Unsupported(_)
        ));
        assert_eq!(rejected.digest().unwrap(), before);
    }
}

#[test]
fn intrinsic_play_prohibitions_apply_to_normal_and_nested_plays() {
    let mut docs = document(&json!({"op":"damage","subjects":"target.1","amount":1_i64}));
    docs["cards"]["unit-spell"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "kind":"static","line":2_i64,"body":{"op":"restrict","subjects":"self","action":"play",
            "condition":{"fn":"eq","args":[{"read":"self.zone"},"ex"]}}
        }));
    docs["cards"]["unit-follower"]["abilities"] = json!([{
        "kind":"activated","line":1_i64,"body":{"op":"seq","steps":[
            {"op":"play_card","subjects":{"zone":"ex","side":"self"},"set_cost":0_i64},
            {"op":"damage","subjects":"opponent.leader","amount":1_i64}
        ]}
    }]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("unit.yaml".into(), docs.to_string())],
        )
        .unwrap(),
    );
    for zone in ["hand", "ex"] {
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"][zone] = json!([{"id":"s","card":"unit-spell"}]);
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "prohibit",
        )
        .unwrap();
        let legal = engine.legal().unwrap();
        assert_eq!(
            legal.iter().any(|choice| choice["card"] == "s"),
            zone == "hand"
        );
        let before = engine.digest().unwrap();
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        assert_eq!(
            step.outcome,
            if zone == "hand" {
                "resolved"
            } else {
                "cannot-play"
            }
        );
        if zone == "ex" {
            assert_eq!(engine.digest().unwrap(), before);
            let nested = engine
                .decide(
                    &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                    "nested",
                )
                .unwrap();
            assert_eq!(nested.outcome, "resolved");
            assert!(
                !nested
                    .events
                    .iter()
                    .any(|event| event["kind"] == "プレイ" && event["object"] == "s")
            );
            assert_eq!(engine.query(View::P1, "P1.ex").unwrap(), Some(json!(["s"])));
            assert_eq!(
                engine.query(View::P1, "P2.leader.life").unwrap(),
                Some(json!(19_i64))
            );
        }
    }
}

#[test]
fn future_restrictions_gate_only_the_named_phase_and_expire_after_use() {
    let body = json!({"op":"seq","steps":[
        {"op":"restrict","subjects":"opponent.leader","action":"play_follower","during":"next-opponent-main"},
        {"op":"restrict","subjects":"target.1","action":"normal_stand","during":"next-controller-start"}
    ]});
    let mut docs = document(&body);
    docs["cards"]["unit-follower"]["abilities"] =
        json!([{"kind":"static","line":1_i64,"body":{"op":"macro","name":"quick"}}]);
    let mut keywords: Value = serde_json::from_str(&registry()).unwrap();
    keywords["keywords"]["quick"] =
        json!({"ja":"クイック","rule":"12.6","expansion":{"op":"keyword","name":"quick"}});
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &keywords.to_string(),
            &[("unit.yaml".into(), docs.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    initial["players"]["P2"]["zones"]["hand"] =
        json!([{"id":"c","card":"unit-follower"},{"id":"t","card":"unit-spell"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d1","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"d2","card":"unit-follower"},{"id":"d3","card":"unit-follower"}]);
    initial["players"]["P2"]["pp"] = json!({"current":2_i64,"max":2_i64});
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "period",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let sampled = Game::from_observation(
        loaded,
        &engine.projection(View::P2).unwrap(),
        "P2",
        "restore-period",
    )
    .unwrap();
    for mut instance in [engine, saved, sampled] {
        instance.decide(&json!({"do":"end-phase"}), "end").unwrap();
        let before_window = instance.legal().unwrap();
        assert!(before_window.iter().any(|choice| choice["card"] == "c"));
        instance
            .decide(&json!({"do":"pass"}), "begin-affected-turn")
            .unwrap();
        let options = instance.legal().unwrap();
        assert!(!options.iter().any(|choice| choice["card"] == "c"));
        assert!(options.iter().any(|choice| choice["card"] == "t"));
        assert_eq!(
            instance.query(View::P2, "P2.field.b.acted").unwrap(),
            Some(json!(true))
        );
        let before = instance.digest().unwrap();
        assert_eq!(
            instance
                .decide(&json!({"do":"play","card":"c"}), "forbidden")
                .unwrap()
                .outcome,
            "cannot-play"
        );
        assert_eq!(instance.digest().unwrap(), before);
        for (decision, node) in [
            (json!({"do":"end-phase"}), "end-affected"),
            (json!({"do":"pass"}), "begin-other"),
            (json!({"do":"end-phase"}), "end-other"),
        ] {
            instance.decide(&decision, node).unwrap();
        }
        let later_quick = instance.legal().unwrap();
        assert!(later_quick.iter().any(|choice| choice["card"] == "c"));
        instance
            .decide(&json!({"do":"pass"}), "begin-later-turn")
            .unwrap();
        let later_main = instance.legal().unwrap();
        assert!(later_main.iter().any(|choice| choice["card"] == "c"));
        assert_eq!(
            instance.query(View::P2, "P2.field.b.acted").unwrap(),
            Some(json!(false))
        );
    }
}

#[test]
fn conditional_restrictions_and_unsupported_periods_do_not_silently_block_play() {
    for period in [Value::Null, json!("unknown-period")] {
        let mut body =
            json!({"op":"restrict","subjects":"self.leader","action":"play","condition":false});
        if !period.is_null() {
            body["during"] = period.clone();
        }
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"s","card":"unit-spell"},{"id":"c","card":"unit-follower"}
        ]);
        let mut engine = Game::new(
            Arc::new(catalog(&body)),
            &initial,
            &Value::Null,
            &Value::Null,
            "condition",
        )
        .unwrap();
        let before = engine.digest().unwrap();
        let result = engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        );
        if period.is_null() {
            assert_eq!(result.unwrap().outcome, "resolved");
            assert!(
                engine
                    .legal()
                    .unwrap()
                    .iter()
                    .any(|option| option["card"] == "c")
            );
        } else {
            assert!(matches!(result.unwrap_err(), EngineFailure::Unsupported(_)));
            assert_eq!(engine.digest().unwrap(), before);
        }
    }
}

#[test]
fn posture_events_and_triggers_follow_actual_batched_changes() {
    let mut docs = document(&json!({"op":"seq","steps":[
        {"op":"act","subjects":{"side":"both","zone":"field"}},
        {"op":"act","subjects":{"side":"both","zone":"field"}}
    ]}));
    docs["cards"]["unit-follower"]["abilities"] = json!([{
        "kind":"trigger","event":"act","subject":"self","line":1_i64,
        "body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}
    }]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("unit.yaml".into(), docs.to_string())],
        )
        .unwrap(),
    );
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &setup(),
        &Value::Null,
        &Value::Null,
        "posture",
    )
    .unwrap();
    let cast = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let acted = cast
        .events
        .iter()
        .filter(|event| event["kind"] == "アクト")
        .collect::<Vec<_>>();
    assert_eq!(acted.len(), 2);
    assert_eq!(acted[0]["group"], acted[1]["group"]);
    let pending = cast
        .events
        .iter()
        .filter(|event| event["kind"] == "待機")
        .collect::<Vec<_>>();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0]["controller"], "P1");
    assert_eq!(pending[0]["event"], json!({"acted":"a"}));
    assert_eq!(pending[1]["controller"], "P2");
    engine
        .decide(
            &json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1_i64}}}),
            "first",
        )
        .unwrap();
    engine
        .decide(
            &json!({"do":"choose-pending","pending":{"ability":{"source":"b","line":1_i64}}}),
            "second",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P1.leader.life").unwrap(),
        Some(json!(19_i64))
    );
    let mut battle = Game::new(
        loaded,
        &setup(),
        &Value::Null,
        &Value::Null,
        "attack-posture",
    )
    .unwrap();
    let attack = battle
        .decide(
            &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
            "attack",
        )
        .unwrap();
    assert_eq!(attack.events[0]["kind"], "アクト");
    assert_eq!(
        attack
            .events
            .iter()
            .filter(|event| event["kind"] == "待機")
            .count(),
        1
    );
    assert!(attack.events.iter().any(|event| event["kind"] == "攻撃"));
}

#[test]
fn invalid_pending_targets_and_distributions_preserve_the_trigger_for_retry() {
    let mut docs = document(&json!({"op":"draw","count":0_i64}));
    docs["cards"]["unit-follower"]["abilities"] = json!([{
        "kind":"trigger","line":1_i64,"event":"attack","subject":"self",
        "targets":[{"key":"1","select":{"side":"opponent","zone":"field"},"min":1_i64,"max":2_i64,"distribute":2_i64}],
        "body":{"op":"damage","subjects":"target.1","amount":2_i64,"split":"1"}
    }]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("unit.yaml".into(), docs.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"c","card":"unit-follower"}));
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "retry",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
            "attack",
        )
        .unwrap();
    let restored = Game::from_observation(
        loaded,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "retry-world",
    )
    .unwrap();
    for mut instance in [engine, restored] {
        let before = instance.digest().unwrap();
        for (targets, amounts) in [
            (json!(["a"]), json!({"a":2_i64})),
            (json!(["b", "c"]), json!({"b":2_i64,"c":0_i64})),
            (json!(["b", "b"]), json!({"b":2_i64})),
        ] {
            let step = instance.decide(&json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1_i64}},"targets":{"1":targets},"distribute":{"1":amounts}}), "invalid").unwrap();
            assert_eq!(step.outcome, "cannot-play");
            assert!(step.events.is_empty());
            assert_eq!(instance.digest().unwrap(), before);
        }
        let choice = instance
            .legal()
            .unwrap()
            .into_iter()
            .find(|choice| choice["targets"]["1"] == json!(["b", "c"]))
            .unwrap();
        assert_eq!(
            instance.decide(&choice, "retry").unwrap().outcome,
            "resolved"
        );
        assert_eq!(
            instance.query(View::P1, "P2.field.b.hp").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P2.field.c.hp").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P1.field.a.acted").unwrap(),
            Some(json!(true))
        );
    }
}

#[test]
fn unplayable_pending_and_explicit_unpaid_costs_cancel_once() {
    for cost in [0_i64, 1_i64, 3_i64] {
        let mut docs = document(&json!({"op":"draw","count":0_i64}));
        let mut trigger = json!({"kind":"trigger","line":1_i64,"event":"attack","subject":"self",
            "targets":[{"key":"1","select":{"side":"opponent","zone":"field"},"min":1_i64,"max":1_i64}],
            "body":{"op":"damage","subjects":"target.1","amount":1_i64}});
        if cost > 0 {
            trigger["costs"] = json!([{"op":"pp","amount":cost}]);
        }
        docs["cards"]["unit-follower"]["abilities"] = json!([trigger]);
        let loaded = Arc::new(
            Catalog::from_documents(
                &snapshot(),
                &registry(),
                &[("unit.yaml".into(), docs.to_string())],
            )
            .unwrap(),
        );
        let mut initial = setup();
        if cost == 0 {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "cancel").unwrap();
        engine
            .decide(
                &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
                "attack",
            )
            .unwrap();
        let mut request =
            json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1_i64}}});
        if cost == 1 {
            request["costs"] = json!("decline");
        }
        let step = engine.decide(&request, "unplayable").unwrap();
        assert_eq!(step.outcome, "pending-cancelled");
        assert_eq!(
            step.events
                .iter()
                .filter(|event| event["kind"] == "待機取消")
                .count(),
            1
        );
        assert_eq!(
            engine.query(View::P1, "P1.pp.current").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(
            engine
                .query(View::P1, "semantic_state.pending_triggers")
                .unwrap(),
            Some(json!([]))
        );
    }
}

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Synthetic selection fixtures must fail on malformed construction."
)]
fn selection_fixture(specs: &Value, body: &Value) -> (Arc<Catalog>, Value) {
    let mut docs = document(body);
    docs["cards"]["unit-spell"]["abilities"][0]["targets"] = specs.clone();
    docs["cards"]["twin"] = docs["cards"]["unit-follower"].clone();
    docs["cards"]["other"] = docs["cards"]["unit-follower"].clone();
    let faces = [("twin","unit-follower"),("other","another-name")].iter().map(|(number,name)|
        json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string()
    ).collect::<Vec<_>>().join("\n");
    let loaded = Arc::new(
        Catalog::from_documents(
            &format!("{}\n{faces}", snapshot()),
            &registry(),
            &[("unit.yaml".into(), docs.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"b","card":"unit-follower"},{"id":"c","card":"twin"},{"id":"d","card":"other"}
    ]);
    (loaded, initial)
}

#[test]
fn play_selections_enumerate_order_without_repeating_names_or_sibling_targets() {
    let first = json!({"key":"1","select":{"side":"opponent","zone":"field"},"min":2_i64,"max":2_i64,"distinct_by":"name","order":true});
    let second = json!({"key":"2","select":{"side":"opponent","zone":"field"},"min":1_i64,"max":1_i64,"different_from":["1"]});
    let (loaded, initial) = selection_fixture(
        &json!([first, second]),
        &json!({"op":"damage","subjects":"target.1","amount":1_i64}),
    );
    let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "selection").unwrap();
    let choices = engine
        .legal()
        .unwrap()
        .into_iter()
        .filter(|choice| choice["do"] == "play")
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 4);
    assert!(
        choices
            .iter()
            .any(|choice| choice["targets"] == json!({"1":["d","b"],"2":["c"]}))
    );
    let before = engine.digest().unwrap();
    for targets in [
        json!({"1":["b","c"],"2":["d"]}),
        json!({"1":["b","d"],"2":["b"]}),
    ] {
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":targets}),
                "invalid",
            )
            .unwrap();
        assert_eq!(step.outcome, "cannot-play");
        assert_eq!(engine.digest().unwrap(), before);
    }
    assert_eq!(
        engine.decide(&choices[0], "valid").unwrap().outcome,
        "resolved"
    );
}

#[test]
fn resolution_selection_chooses_as_many_distinct_names_as_possible_and_restores() {
    let body = json!({"op":"seq","steps":[
        {"op":"select","select":{"side":"opponent","zone":"field"},"min":3_i64,"max":3_i64,"distinct_by":"name","bind":"selected"},
        {"op":"damage","subjects":"selected","amount":1_i64}
    ]});
    let (loaded, initial) = selection_fixture(&json!([]), &body);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "distinct",
    )
    .unwrap();
    assert_eq!(
        engine
            .decide(&json!({"do":"play","card":"s"}), "cast")
            .unwrap()
            .outcome,
        "paused"
    );
    assert_eq!(
        engine.legal().unwrap(),
        vec![
            json!({"do":"resolve-choice","select":["b","d"]}),
            json!({"do":"resolve-choice","select":["c","d"]})
        ]
    );
    let restored = Game::from_observation(
        loaded,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "restore-distinct",
    )
    .unwrap();
    for mut instance in [engine, restored] {
        let before = instance.digest().unwrap();
        assert_eq!(
            instance
                .decide(
                    &json!({"do":"resolve-choice","select":["d","d"]}),
                    "duplicates"
                )
                .unwrap()
                .outcome,
            "cannot-play"
        );
        assert_eq!(instance.digest().unwrap(), before);
        assert_eq!(
            instance
                .decide(
                    &json!({"do":"resolve-choice","select":["d","b"]}),
                    "reverse-set"
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        assert_eq!(
            instance.query(View::P1, "P2.field.b.hp").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P2.field.c.hp").unwrap(),
            Some(json!(3_i64))
        );
    }
}

#[test]
fn unknown_distinct_properties_fail_without_creating_an_empty_prompt() {
    let body = json!({"op":"select","select":{"side":"opponent","zone":"field"},"min":0_i64,"max":2_i64,"distinct_by":"unknown-property","bind":"selected"});
    let mut engine = game(&body);
    let before = engine.digest().unwrap();
    assert!(matches!(
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "unsupported"
            )
            .unwrap_err(),
        EngineFailure::Unsupported(_)
    ));
    assert_eq!(engine.digest().unwrap(), before);
}

#[test]
fn a_single_flat_target_list_can_require_different_counts_from_two_groups() {
    let specs = json!([{"key":"1","select":{"union":[
        {"side":"self","zone":"field"},{"side":"opponent","zone":"field"}
    ]},"min":1_i64,"max":2_i64,"constraint":{"fn":"eq","args":[{"count":{"from":"target.1","side":"opponent"}},1_i64]}}]);
    let body = json!({"op":"seq","steps":[
        {"op":"damage","subjects":{"from":"target.1","side":"opponent"},"amount":1_i64},
        {"op":"modify","subjects":{"from":"target.1","side":"self"},"power":2_i64}
    ]});
    let (loaded, initial) = selection_fixture(&specs, &body);
    let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "flat-group").unwrap();
    let choices = engine
        .legal()
        .unwrap()
        .into_iter()
        .filter(|choice| choice["do"] == "play")
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 6);
    assert!(
        choices
            .iter()
            .all(|choice| choice["targets"].as_object().unwrap().len() == 1)
    );
    let before = engine.digest().unwrap();
    for selected in [json!(["a"]), json!(["b", "c"])] {
        assert_eq!(
            engine
                .decide(
                    &json!({"do":"play","card":"s","targets":{"1":selected}}),
                    "invalid-groups"
                )
                .unwrap()
                .outcome,
            "cannot-play"
        );
        assert_eq!(engine.digest().unwrap(), before);
    }
    assert_eq!(
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b","a"]}}),
                "valid-groups"
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    assert_eq!(
        engine.query(View::P1, "P2.field.b.hp").unwrap(),
        Some(json!(2_i64))
    );
    assert_eq!(
        engine.query(View::P1, "P1.field.a.power").unwrap(),
        Some(json!(4_i64))
    );
}

#[test]
fn resolution_flip_payment_requires_all_materials_and_groups_the_costs() {
    let body = json!({"op":"pay","cost_selections":[{"key":"1","select":{"side":"self","zone":"evolve_deck"},"min":2_i64,"max":2_i64}],
        "costs":[{"op":"flip","subjects":"cost.1","face":"down"}],
        "then":{"op":"damage","subjects":"target.1","amount":1_i64}});
    let loaded = Arc::new(catalog(&body));
    for count in 0..=3_u32 {
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!(
            (0..count)
                .map(|n| json!({"id":format!("e{n}"),"card":"unit-follower","face_up":true}))
                .collect::<Vec<_>>()
        );
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "payment",
        )
        .unwrap();
        assert_eq!(
            engine
                .decide(
                    &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                    "cast"
                )
                .unwrap()
                .outcome,
            "paused"
        );
        let choices = engine.legal().unwrap();
        if count < 2 {
            assert_eq!(
                choices,
                vec![json!({"do":"resolve-choice","choice":"decline"})]
            );
            engine.decide(&choices[0], "decline").unwrap();
            assert_eq!(
                engine.query(View::P1, "P2.field.b.hp").unwrap(),
                Some(json!(3_i64))
            );
            continue;
        }
        assert_eq!(choices.len(), if count == 2 { 2 } else { 4 });
        let before = engine.digest().unwrap();
        for selected in [json!(["e0"]), json!(["e0", "e0"])] {
            assert_eq!(
                engine
                    .decide(
                        &json!({"do":"resolve-choice","choice":"execute","select":selected}),
                        "bad-cost"
                    )
                    .unwrap()
                    .outcome,
                "cannot-play"
            );
            assert_eq!(engine.digest().unwrap(), before);
        }
        let mut restored = Game::from_observation(
            Arc::clone(&loaded),
            &engine.projection(View::P1).unwrap(),
            "P1",
            "restore-payment",
        )
        .unwrap();
        let paid = restored
            .decide(
                &json!({"do":"resolve-choice","choice":"execute","select":["e1","e0"]}),
                "pay",
            )
            .unwrap();
        assert_eq!(paid.outcome, "resolved");
        let flips = paid
            .events
            .iter()
            .filter(|event| event["kind"] == "表向き／裏向き")
            .collect::<Vec<_>>();
        assert_eq!(flips.len(), 2);
        assert_eq!(flips[0]["group"], flips[1]["group"]);
        assert_eq!(
            restored
                .query(View::P1, "P1.evolve_deck.e0.face_up")
                .unwrap(),
            Some(json!(false))
        );
        assert_eq!(
            restored
                .query(View::P1, "P1.evolve_deck.e1.face_up")
                .unwrap(),
            Some(json!(false))
        );
        assert_eq!(
            restored.query(View::P1, "P2.field.b.hp").unwrap(),
            Some(json!(2_i64))
        );
    }
}

#[test]
fn payment_bindings_restore_after_a_paused_body_without_repaying_additional_costs() {
    let body = json!({"op":"seq","steps":[
        {"op":"pay","cost_selections":[{"key":"1","select":{"side":"self","zone":"evolve_deck"},"min":1_i64,"max":1_i64}],
            "costs":[{"op":"flip","subjects":"cost.1","face":"down"},{"op":"pp","amount":1_i64}],
            "then":{"op":"optional","then":{"op":"modify","subjects":"cost.1","power":1_i64}}},
        {"op":"modify","subjects":"cost.1","power":2_i64}
    ]});
    let mut program = document(&body);
    let ability = &mut program["cards"]["unit-spell"]["abilities"][0];
    ability["cost_selections"] =
        json!([{"key":"1","select":{"side":"self","zone":"field"},"min":1_i64,"max":1_i64}]);
    ability["costs"] = json!([{"op":"reveal","subjects":"cost.1","to":"all"}]);
    ability["additional_costs"] = json!([{"key":"extra","costs":[{"op":"pp","amount":1_i64}]}]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("scope.yaml".into(), program.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["pp"] = json!({"current":4_i64,"max":4_i64});
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"e0","card":"unit-follower","face_up":true}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "scope",
    )
    .unwrap();
    engine.decide(&json!({"do":"play","card":"s","targets":{"1":["b"]},"costs":{"1":["a"]},"optional_costs":{"additional":{"pp":1_i64}}}), "cast").unwrap();
    assert_eq!(
        engine.query(View::P1, "P1.pp.current").unwrap(),
        Some(json!(2_i64))
    );
    assert_eq!(
        engine
            .decide(
                &json!({"do":"resolve-choice","choice":"execute","select":["e0"]}),
                "pay"
            )
            .unwrap()
            .outcome,
        "paused"
    );
    assert_eq!(
        engine.query(View::P1, "P1.pp.current").unwrap(),
        Some(json!(1_i64))
    );
    let restored = Game::from_observation(
        loaded,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "restore-scope",
    )
    .unwrap();
    for mut instance in [engine, restored] {
        assert_eq!(
            instance
                .decide(&json!({"do":"resolve-choice","choice":"execute"}), "then")
                .unwrap()
                .outcome,
            "resolved"
        );
        assert_eq!(
            instance.query(View::P1, "P1.field.a.power").unwrap(),
            Some(json!(4_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P1.evolve_deck.e0.power").unwrap(),
            Some(json!(3_i64))
        );
    }
}

#[test]
fn private_payment_selections_stay_hidden_and_unsupported_costs_rollback() {
    let body = json!({"op":"pay","cost_selections":[{"key":"1","select":{"side":"self","zone":"hand"},"min":1_i64,"max":1_i64}],
        "costs":[{"op":"discard","subjects":"cost.1"}],
        "then":{"op":"damage","subjects":"target.1","amount":1_i64}});
    let loaded = Arc::new(catalog(&body));
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"private-material","card":"unit-follower"}));
    let mut engine = Game::new(
        loaded,
        &initial,
        &Value::Null,
        &Value::Null,
        "private-payment",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert!(
        !engine
            .projection(View::P2)
            .unwrap()
            .to_string()
            .contains("private-material")
    );
    assert!(engine.legal().unwrap().contains(
        &json!({"do":"resolve-choice","choice":"execute","select":["private-material"]})
    ));
    engine
        .decide(
            &json!({"do":"resolve-choice","choice":"execute","select":["private-material"]}),
            "discard",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P1.cemetery").unwrap(),
        Some(json!(["private-material", "s"]))
    );
    let mut unsupported = body;
    unsupported["cost_selections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"2","select":{"side":"self","zone":"field"},"min":1_i64,"max":1_i64}));
    let mut denied = game(&unsupported);
    let before = denied.digest().unwrap();
    assert!(matches!(
        denied.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "unsupported"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(denied.digest().unwrap(), before);
}

#[test]
fn random_materials_remain_private_and_scripts_restore_without_partial_consumption() {
    let body = json!({"op":"seq","steps":[
        {"op":"random","select":{"side":"opponent","zone":"hand"},"count":2_i64,"bind":"picked"},
        {"op":"optional","then":{"op":"discard","subjects":"picked"}},
        {"op":"random","select":{"side":"opponent","zone":"hand"},"count":2_i64,"bind":"last"},
        {"op":"discard","subjects":"last"}
    ]});
    let loaded = Arc::new(catalog(&body));
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["hand"] = json!(
        (0..3_u32)
            .map(|n| json!({"id":format!("private-{n}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    for script in [
        json!({"n":1_i64,"from":"P1.hand","result":["private-0","private-1"]}),
        json!({"n":2_i64,"from":"P2.hand","result":["private-0","private-1"]}),
        json!({"n":1_i64,"from":"P2.hand","result":["private-0","private-0"]}),
        json!({"n":1_i64,"from":"P2.hand","result":["private-0","b"]}),
    ] {
        let mut invalid = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &json!({"random_selections":[script]}),
            "invalid-random",
        )
        .unwrap();
        let before = invalid.digest().unwrap();
        invalid
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "bad-script",
            )
            .unwrap_err();
        assert_eq!(invalid.digest().unwrap(), before);
    }
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &json!({"random_selections":[
            {"n":1_i64,"from":"P2.hand","result":["private-1","private-0"]},
            {"n":2_i64,"from":"P2.hand","result":["private-2"]}
        ]}),
        "private-random",
    )
    .unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast"
            )
            .unwrap()
            .outcome,
        "paused"
    );
    let packet = engine.projection(View::P1).unwrap();
    assert!(!packet.to_string().contains("private-"));
    assert!(packet.get("continuation").is_none());
    assert!(matches!(
        Game::from_observation(loaded, &packet, "P1", "sample"),
        Err(EngineFailure::Unsupported(_))
    ));
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    assert_eq!(
        restored
            .decide(
                &json!({"do":"resolve-choice","choice":"execute"}),
                "discard"
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    assert_eq!(
        restored.query(View::P1, "P2.hand_count").unwrap(),
        Some(json!(0_i64))
    );
    assert_eq!(
        restored.query(View::P1, "P2.cemetery").unwrap(),
        Some(json!(["private-1", "private-0", "private-2"]))
    );
}

#[test]
fn unscripted_random_selection_is_reproducible_and_never_ignores_anonymous_cards() {
    let body = json!({"op":"seq","steps":[
        {"op":"random","select":{"side":"opponent","zone":"hand"},"count":2_i64,"bind":"picked"},
        {"op":"discard","subjects":"picked"}
    ]});
    let loaded = Arc::new(catalog(&body));
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["hand"] = json!(
        (0..4_u32)
            .map(|n| json!({"id":format!("h{n}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "sample-selection",
    )
    .unwrap();
    let mut identical = engine.clone();
    for instance in [&mut engine, &mut identical] {
        instance
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        assert_eq!(
            instance.query(View::P1, "P2.hand_count").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P2.cemetery_count").unwrap(),
            Some(json!(2_i64))
        );
    }
    assert_eq!(engine.digest().unwrap(), identical.digest().unwrap());
    initial["players"]["P2"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"filler":1_i64}));
    let mut opaque = Game::new(loaded, &initial, &Value::Null, &Value::Null, "opaque").unwrap();
    let before = opaque.digest().unwrap();
    assert!(matches!(
        opaque.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(opaque.digest().unwrap(), before);
}

#[test]
fn rerolls_replace_the_result_and_permissions_reset_for_each_new_roll() {
    let body = json!({"op":"per","count":2_i64,"body":{"op":"seq","steps":[
        {"op":"dice","count":1_i64,"bind":"die"},
        {"op":"damage","subjects":"opponent.leader","amount":{"read":"die"}}
    ]}});
    let mut program = document(&body);
    program["cards"]["unit-follower"]["abilities"] = json!([{"kind":"static","line":1_i64,"body":{"op":"allow_reroll","side":"self","count":1_i64}}]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("reroll.yaml".into(), program.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"second","card":"unit-follower"}));
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &json!({"dice":[1_i64,2_i64,3_i64,4_i64]}),
        "dice",
    )
    .unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast"
            )
            .unwrap()
            .outcome,
        "paused"
    );
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(packet["continuation"]["frame"]["values"]["die"], 1_i64);
    for key in ["random", "random_cursors", "rng"] {
        assert!(packet.get(key).is_none());
        assert!(!packet.to_string().contains(&format!("\"{key}\":")));
    }
    let sampled = Game::from_observation(loaded, &packet, "P1", "dice-sample").unwrap();
    assert_eq!(sampled.legal().unwrap(), engine.legal().unwrap());
    assert_eq!(
        engine
            .decide(
                &json!({"do":"resolve-choice","choice":"execute"}),
                "reroll-one"
            )
            .unwrap()
            .outcome,
        "paused"
    );
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    assert_eq!(
        restored
            .decide(
                &json!({"do":"resolve-choice","choice":"execute"}),
                "reroll-two"
            )
            .unwrap()
            .outcome,
        "paused"
    );
    assert_eq!(
        restored.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(17_i64))
    );
    assert_eq!(
        restored.projection(View::P1).unwrap()["continuation"]["frame"]["values"]["die"],
        4_i64
    );
    assert_eq!(
        restored
            .decide(
                &json!({"do":"resolve-choice","choice":"decline"}),
                "keep-second-roll"
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    assert_eq!(
        restored.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(13_i64))
    );
}

#[test]
fn invalid_dice_scripts_rollback_and_unscripted_rolls_stay_in_range() {
    let body = json!({"op":"seq","steps":[{"op":"dice","count":1_i64,"bind":"die"},{"op":"damage","subjects":"opponent.leader","amount":{"read":"die"}}]});
    let loaded = Arc::new(catalog(&body));
    for script in [json!([]), json!([0_i64]), json!([7_i64]), json!([1.5_f64])] {
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &setup(),
            &Value::Null,
            &json!({"dice":script}),
            "invalid-die",
        )
        .unwrap();
        let before = engine.digest().unwrap();
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap_err();
        assert_eq!(engine.digest().unwrap(), before);
    }
    for n in 0..8_u32 {
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &setup(),
            &Value::Null,
            &Value::Null,
            &format!("die-{n}"),
        )
        .unwrap();
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        let rolled = step
            .events
            .iter()
            .find(|event| event["kind"] == "サイコロ")
            .unwrap()["result"]
            .as_i64()
            .unwrap();
        assert!((1..=6).contains(&rolled));
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(20_i64 - rolled))
        );
    }
}

#[expect(
    clippy::indexing_slicing,
    reason = "The fixture constructs the cards map before adding synthetic token prints."
)]
fn token_fixture(body: &Value) -> (String, Value) {
    let mut facts = snapshot();
    let mut docs = document(body);
    for (number, name, preferred) in [
        ("token-a", "shared-token", false),
        ("token-z", "shared-token", true),
        ("token-b", "other-token", false),
    ] {
        facts.push('\n');
        facts.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー・トークン","cost":"1","power":"1","hp":"2","traits":[],"text":null,"sections":[]}]}).to_string());
        docs["cards"][number] = json!({"status":"complete","review":"synthetic","abilities":[],"token_template":preferred});
    }
    (facts, docs)
}

#[test]
fn token_templates_require_one_complete_representative_for_ambiguous_names() {
    let (facts, docs) = token_fixture(&json!({"op":"draw","count":0_i64}));
    for change in ["missing", "duplicate", "not-token", "partial"] {
        let mut invalid = docs.clone();
        match change {
            "missing" => invalid["cards"]["token-z"]["token_template"] = json!(false),
            "duplicate" => invalid["cards"]["token-a"]["token_template"] = json!(true),
            "not-token" => invalid["cards"]["unit-follower"]["token_template"] = json!(true),
            _ => invalid["cards"]["token-z"]["status"] = json!("partial"),
        }
        Catalog::from_documents(
            &facts,
            &registry(),
            &[("tokens".into(), invalid.to_string())],
        )
        .unwrap_err();
    }
    Catalog::from_documents(&facts, &registry(), &[("tokens".into(), docs.to_string())]).unwrap();
}

#[test]
fn token_capacity_selects_print_multisets_before_allocating_and_restores_text_order() {
    let body = json!({"op":"seq","steps":[{"op":"create","tokens":[{"name":"shared-token","count":1_i64},{"name":"other-token","count":1_i64},{"name":"shared-token","count":1_i64}],"to":"field","bind":"made"},{"op":"modify","subjects":"made","power":1_i64}]});
    let (facts, docs) = token_fixture(&body);
    let loaded = Arc::new(
        Catalog::from_documents(&facts, &registry(), &[("tokens".into(), docs.to_string())])
            .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"a","card":"unit-follower"},{"id":"c","card":"unit-follower"},{"id":"d","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "tokens",
    )
    .unwrap();
    let start = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(start.outcome, "paused");
    let state = serde_json::to_value(&engine).unwrap();
    assert_eq!(state["state"]["next_object"], 1_i64);
    assert!(state["state"]["objects"].get("new-1").is_none());
    let choices = engine.legal().unwrap();
    assert_eq!(choices.len(), 2);
    assert!(choices.contains(&json!({"do":"resolve-choice","select":["token-z","token-z"]})));
    assert!(choices.contains(&json!({"do":"resolve-choice","select":["token-z","token-b"]})));
    let saved = engine.digest().unwrap();
    let rejected = engine
        .decide(
            &json!({"do":"resolve-choice","select":["token-z","token-z","token-z"]}),
            "invalid",
        )
        .unwrap();
    assert_eq!(rejected.outcome, "cannot-play");
    assert_eq!(engine.digest().unwrap(), saved);
    let mut restored: Game = serde_json::from_value(state).unwrap();
    let mut sampled = Game::from_observation(
        loaded,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "sample",
    )
    .unwrap();
    for instance in [&mut engine, &mut restored, &mut sampled] {
        assert_eq!(instance.legal().unwrap(), choices);
        let done = instance
            .decide(
                &json!({"do":"resolve-choice","select":["token-b","token-z"]}),
                "choose",
            )
            .unwrap();
        assert_eq!(done.outcome, "resolved");
        let entered = done
            .events
            .iter()
            .filter(|event| event["kind"] == "場に出す")
            .collect::<Vec<_>>();
        assert_eq!(entered.len(), 2);
        assert_eq!(entered[0]["object"], "new-1");
        assert_eq!(entered[0]["card"], "token-z");
        assert_eq!(entered[1]["object"], "new-2");
        assert_eq!(entered[1]["card"], "token-b");
        assert_eq!(entered[0]["group"], entered[1]["group"]);
        assert!(done.events.iter().any(|event| event["object"] == "s"
            && event["to"] == "P1.cemetery"
            && event["by"] == "rule-10.6.2.8.3"));
        assert_eq!(
            instance.query(View::P1, "P1.field.new-1.power").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P1.field.new-2.power").unwrap(),
            Some(json!(2_i64))
        );
    }
}

#[test]
fn token_capacity_keeps_single_and_empty_choices_without_allocating_rejected_tokens() {
    let body = json!({"op":"seq","steps":[{"op":"create","name":"shared-token","count":2_i64,"to":"field"},{"op":"create","name":"other-token","count":1_i64,"to":"ex"},{"op":"create","name":"unknown-zero-token","count":0_i64,"to":"field"}]});
    let (facts, docs) = token_fixture(&body);
    let loaded = Arc::new(
        Catalog::from_documents(&facts, &registry(), &[("tokens".into(), docs.to_string())])
            .unwrap(),
    );
    for available in [0_u32, 1_u32] {
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["field"] = json!(
            (available..5_u32)
                .map(|n| json!({"id":format!("f{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "tokens",
        )
        .unwrap();
        assert_eq!(
            engine
                .decide(
                    &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                    "cast"
                )
                .unwrap()
                .outcome,
            "paused"
        );
        let selected = if available == 0 {
            Vec::new()
        } else {
            vec!["token-z"]
        };
        let choice = json!({"do":"resolve-choice","select":selected});
        assert_eq!(engine.legal().unwrap(), vec![choice.clone()]);
        assert_eq!(
            engine.decide(&choice, "choose").unwrap().outcome,
            "resolved"
        );
        let state = serde_json::to_value(&engine).unwrap();
        assert_eq!(state["state"]["next_object"], available.saturating_add(2));
        assert_eq!(
            engine.query(View::P1, "P1.ex").unwrap(),
            Some(json!([format!("new-{}", available.saturating_add(1))]))
        );
    }
}

#[test]
fn transformation_banishes_and_erases_before_creating_a_fresh_batch() {
    let body = json!({"op":"seq","steps":[{"op":"transform","subjects":"target.1","names":["shared-token","other-token"],"bind":"changed"},{"op":"modify","subjects":"changed","power":1_i64}]});
    let (facts, mut docs) = token_fixture(&body);
    docs["cards"]["unit-spell"]["abilities"][0]["targets"][0] = json!({"key":"1","select":{"zone":"ex","side":"self"},"min":2_i64,"max":2_i64,"order":true});
    let loaded = Arc::new(
        Catalog::from_documents(&facts, &registry(), &[("tokens".into(), docs.to_string())])
            .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"x","card":"token-a","state":{"power":9_i64,"hp":9_i64,"keywords":["guard"]}},{"id":"y","card":"token-a"}]);
    let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "transform").unwrap();
    let done = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["x","y"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(done.outcome, "resolved");
    let phase = |kind: &str| {
        done.events
            .iter()
            .filter(|event| event["kind"] == kind)
            .collect::<Vec<_>>()
    };
    let banished = phase("消滅");
    let erased = phase("消去");
    let created = done
        .events
        .iter()
        .filter(|event| event["to"] == "P1.ex")
        .collect::<Vec<_>>();
    assert_eq!(banished.len(), 2);
    assert_eq!(erased.len(), 2);
    assert_eq!(created.len(), 2);
    assert_eq!(banished[0]["group"], banished[1]["group"]);
    assert_eq!(erased[0]["group"], erased[1]["group"]);
    assert_eq!(created[0]["group"], created[1]["group"]);
    let groups = [
        banished[0]["group"].as_u64().unwrap(),
        erased[0]["group"].as_u64().unwrap(),
        created[0]["group"].as_u64().unwrap(),
    ];
    assert!(groups[0] < groups[1] && groups[1] < groups[2]);
    assert!(created.iter().all(|event| event.get("card").is_none()));
    assert_eq!(
        engine.query(View::P1, "P1.ex.new-1.name").unwrap(),
        Some(json!("shared-token"))
    );
    assert_eq!(
        engine.query(View::P1, "P1.ex.new-2.name").unwrap(),
        Some(json!("other-token"))
    );
    assert_eq!(created[0]["source"], "s");
    assert!(phase("場に出す").is_empty());
    assert_eq!(
        engine.query(View::P1, "P1.banish").unwrap(),
        Some(json!([]))
    );
    assert_eq!(
        engine.query(View::P1, "P1.ex").unwrap(),
        Some(json!(["new-1", "new-2"]))
    );
    assert_eq!(
        engine.query(View::P1, "P1.ex.new-1.power").unwrap(),
        Some(json!(2_i64))
    );
    assert_eq!(
        engine.query(View::P1, "P1.ex.new-1.hp").unwrap(),
        Some(json!(2_i64))
    );
    let state = serde_json::to_value(&engine).unwrap();
    assert_eq!(state["state"]["objects"]["x"]["zone"], "void");
    assert_eq!(
        state["state"]["objects"]["new-1"]["state"]["keywords"],
        json!([])
    );
}

#[test]
fn transformations_keep_original_pairing_after_prohibition_replacement_or_stale_targets() {
    for case in ["stale", "prohibited", "replaced"] {
        let first = match case {
            "stale" => json!({"op":"move","subjects":"b","to":"ex"}),
            "prohibited" => {
                json!({"op":"restrict","subjects":"b","action":"banish","until":"game"})
            }
            _ => json!({"op":"draw","count":0_i64}),
        };
        let body = json!({"op":"seq","steps":[first,{"op":"transform","subjects":"target.1","names":["shared-token","other-token"],"bind":"changed"},{"op":"modify","subjects":"changed","power":1_i64}]});
        let (facts, mut docs) = token_fixture(&body);
        docs["cards"]["unit-spell"]["abilities"][0]["targets"][0]["min"] = json!(2_i64);
        docs["cards"]["unit-spell"]["abilities"][0]["targets"][0]["max"] = json!(2_i64);
        docs["cards"]["unit-spell"]["abilities"][0]["targets"][0]["order"] = json!(true);
        docs["cards"]["token-b"]["abilities"] =
            json!([{"kind":"static","line":1_i64,"body":{"op":"keyword","name":"guard"}}]);
        docs["cards"]["token-z"]["abilities"] =
            json!([{"kind":"static","line":1_i64,"body":{"op":"keyword","name":"stack"}}]);
        let mut keywords: Value = serde_json::from_str(&registry()).unwrap();
        keywords["keywords"]["stack"] =
            json!({"ja":"スタック","rule":"13.3.2","expansion":{"op":"keyword","name":"stack"}});
        let loaded = Arc::new(
            Catalog::from_documents(
                &facts,
                &keywords.to_string(),
                &[("tokens".into(), docs.to_string())],
            )
            .unwrap(),
        );
        let mut initial = setup();
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"b","card":if case == "replaced" {"token-z"} else {"unit-follower"},"state":{"counters":{"stack_counter":2_i64}}},{"id":"c","card":"unit-follower"}]);
        let mut engine =
            Game::new(loaded, &initial, &Value::Null, &Value::Null, "transform").unwrap();
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b","c"]}}),
                "cast",
            )
            .unwrap();
        assert_eq!(step.outcome, "paused");
        assert!(
            step.events
                .iter()
                .any(|event| event["kind"] == "消滅" && event["object"] == "c")
        );
        assert!(
            !step
                .events
                .iter()
                .any(|event| event["kind"] == "消滅" && event["object"] == "b")
        );
        assert_eq!(engine.projection(View::P2).unwrap()["awaiting"]["by"], "P2");
        let state = serde_json::to_value(&engine).unwrap();
        assert_eq!(state["state"]["objects"]["new-1"]["card"], "token-b");
        assert_eq!(state["state"]["objects"]["new-1"]["owner"], "P2");
        let mut restored: Game = serde_json::from_value(state).unwrap();
        for instance in [&mut engine, &mut restored] {
            assert_eq!(
                instance
                    .decide(
                        &json!({"do":"place-acted","object":"new-1","acted":true}),
                        "place"
                    )
                    .unwrap()
                    .outcome,
                "resolved"
            );
            assert_eq!(
                instance.query(View::P1, "P2.field.new-1.acted").unwrap(),
                Some(json!(true))
            );
            assert_eq!(
                instance.query(View::P1, "P2.field.new-1.power").unwrap(),
                Some(json!(2_i64))
            );
            assert_eq!(
                instance.query(View::P1, "P2.banish").unwrap(),
                Some(json!(["c"]))
            );
        }
    }
}

#[test]
fn unsupported_mixed_origin_transformations_roll_back_payment_and_every_object() {
    let (facts, docs) = token_fixture(
        &json!({"op":"transform","subjects":{"union":["a","target.1"]},"name":"other-token"}),
    );
    let loaded = Arc::new(
        Catalog::from_documents(&facts, &registry(), &[("tokens".into(), docs.to_string())])
            .unwrap(),
    );
    let mut engine = Game::new(loaded, &setup(), &Value::Null, &Value::Null, "transform").unwrap();
    let before = engine.digest().unwrap();
    assert!(matches!(
        engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(engine.digest().unwrap(), before);
}

#[test]
fn pp_maximum_changes_clamp_both_values_and_report_only_actual_changes() {
    for (maximum, amount, expected_maximum, expected_current, changes) in [
        (9_i64, 3_i64, 10_i64, 8_i64, true),
        (10, 2, 10, 8, false),
        (9, -6, 3, 3, true),
        (9, -20, 0, 0, true),
        (9, 0, 9, 8, false),
    ] {
        let body = json!({"op":"if_done","attempt":{"op":"max_pp","amount":amount},"then":{"op":"damage","subjects":"opponent.leader","amount":1_i64}});
        let mut initial = setup();
        initial["players"]["P1"]["pp"] = json!({"current":9_i64,"max":maximum});
        let mut engine = Game::new(
            Arc::new(catalog(&body)),
            &initial,
            &Value::Null,
            &Value::Null,
            "resource",
        )
        .unwrap();
        let step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        assert_eq!(step.outcome, "resolved");
        assert_eq!(
            engine.query(View::P1, "P1.pp.max").unwrap(),
            Some(json!(expected_maximum))
        );
        assert_eq!(
            engine.query(View::P1, "P1.pp.current").unwrap(),
            Some(json!(expected_current))
        );
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(if changes { 19_i64 } else { 20_i64 }))
        );
        let events = step
            .events
            .iter()
            .filter(|event| event["kind"] == "PP最大値変化")
            .collect::<Vec<_>>();
        assert_eq!(events.len(), usize::from(changes));
        if changes {
            assert_eq!(events[0]["delta"], expected_maximum.saturating_sub(maximum));
        }
    }
}

#[test]
fn ep_has_no_gameplay_cap_and_batched_deltas_survive_resume() {
    let body = json!({"op":"seq","steps":[
        {"op":"ep","side":"both","amount":4_i64},
        {"op":"optional","then":{"op":"ep","side":"opponent","amount":-20_i64}},
        {"op":"if_done","attempt":{"op":"ep","side":"opponent","amount":-1_i64},"then":{"op":"damage","subjects":"opponent.leader","amount":5_i64}}
    ]});
    let loaded = Arc::new(catalog(&body));
    let mut initial = setup();
    initial["players"]["P1"]["ep"] = json!(3_i64);
    initial["players"]["P2"]["ep"] = json!(12_i64);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "resource",
    )
    .unwrap();
    let step = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(step.outcome, "paused");
    assert_eq!(engine.query(View::P1, "P1.ep").unwrap(), Some(json!(7_i64)));
    assert_eq!(
        engine.query(View::P1, "P2.ep").unwrap(),
        Some(json!(16_i64))
    );
    let changes = step
        .events
        .iter()
        .filter(|event| event["kind"] == "EP変化")
        .collect::<Vec<_>>();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["group"], changes[1]["group"]);
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let mut sampled = Game::from_observation(
        loaded,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "sample",
    )
    .unwrap();
    for instance in [&mut engine, &mut restored, &mut sampled] {
        let done = instance
            .decide(
                &json!({"do":"resolve-choice","choice":"execute"}),
                "decrease",
            )
            .unwrap();
        assert_eq!(done.outcome, "resolved");
        assert_eq!(
            instance.query(View::P1, "P2.ep").unwrap(),
            Some(json!(0_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(20_i64))
        );
        let decreased = done
            .events
            .iter()
            .filter(|event| event["kind"] == "EP変化")
            .collect::<Vec<_>>();
        assert_eq!(decreased.len(), 1);
        assert_eq!(decreased[0]["delta"], -16_i64);
    }
    let mut overflow = game(
        &json!({"op":"seq","steps":[{"op":"ep","side":"self","amount":i64::MAX},{"op":"ep","side":"self","amount":1_i64}]}),
    );
    let before = overflow.digest().unwrap();
    assert!(matches!(
        overflow.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "overflow"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(overflow.digest().unwrap(), before);
}

#[expect(
    clippy::indexing_slicing,
    reason = "The setup fixture already constructs both player zone maps."
)]
fn turn_setup() -> Value {
    let mut initial = setup();
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u32..4_u32)
                .map(|n| json!({"id":format!("{seat}-d{n}"),"card":if n < 2 {"unit-follower"} else {"unit-spell"}}))
                .collect::<Vec<_>>()
        );
    }
    initial["room"] = json!({"open_decklists":true});
    initial["players"]["P1"]["deck_list"] =
        json!([{"card":"unit-follower","count":3_i64},{"card":"unit-spell","count":3_i64}]);
    initial["players"]["P2"]["deck_list"] =
        json!([{"card":"unit-follower","count":3_i64},{"card":"unit-spell","count":2_i64}]);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":true});
    initial
}

#[expect(
    clippy::unwrap_used,
    reason = "The fixture ends a turn with no pending abilities or guards."
)]
fn finish_turn(engine: &mut Game) -> Vec<Value> {
    engine.decide(&json!({"do":"end-phase"}), "end").unwrap();
    engine.decide(&json!({"do":"pass"}), "pass").unwrap().events
}

#[test]
fn repeated_skip_costs_apply_to_separate_future_turns_and_roundtrip_publicly() {
    let mut docs = document(&json!({"op":"draw","count":0_i64}));
    docs["cards"]["unit-spell"]["abilities"][0]["costs"] = json!([{"op":"skip_turn","side":"self","count":1_i64},{"op":"skip_turn","side":"self","count":1_i64}]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("turns".into(), docs.to_string())],
        )
        .unwrap(),
    );
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &turn_setup(),
        &Value::Null,
        &Value::Null,
        "turns",
    )
    .unwrap();
    let start = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(start.outcome, "resolved");
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(
        packet["semantic_state"]["turn_schedule"]["skipped"],
        json!([{"player":"P1","count":1_i64},{"player":"P1","count":1_i64}])
    );
    let mut sampled = Game::from_observation(loaded, &packet, "P1", "sample").unwrap();
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    for instance in [&mut engine, &mut sampled, &mut restored] {
        finish_turn(instance);
        for expected_elapsed in [4_i64, 5_i64] {
            let events = finish_turn(instance);
            assert_eq!(
                instance.query(View::P1, "turn.active").unwrap(),
                Some(json!("P2"))
            );
            assert_eq!(
                instance.query(View::P1, "turn.elapsed_turns.P1").unwrap(),
                Some(json!(3_i64))
            );
            assert_eq!(
                instance.query(View::P1, "turn.elapsed_turns.P2").unwrap(),
                Some(json!(expected_elapsed))
            );
            assert_eq!(
                instance.query(View::P1, "P1.pp.current").unwrap(),
                Some(json!(1_i64))
            );
            assert_eq!(
                instance.query(View::P1, "P1.field.a.acted").unwrap(),
                Some(json!(true))
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event["kind"] == "ターンスキップ")
                    .count(),
                1
            );
            assert!(
                !events
                    .iter()
                    .any(|event| event["kind"] == "引く" && event["player"] == "P1")
            );
        }
        finish_turn(instance);
        assert_eq!(
            instance.query(View::P1, "turn.active").unwrap(),
            Some(json!("P1"))
        );
        assert_eq!(
            instance.query(View::P1, "turn.elapsed_turns.P1").unwrap(),
            Some(json!(4_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P1.field.a.acted").unwrap(),
            Some(json!(false))
        );
        assert_eq!(
            instance.query(View::P1, "P1.hand_count").unwrap(),
            Some(json!(1_i64))
        );
    }
}

#[test]
fn extra_turns_run_latest_first_then_resume_the_original_normal_player() {
    let body = json!({"op":"seq","steps":[{"op":"extra_turn","side":"self","count":1_i64},{"op":"extra_turn","side":"opponent","count":1_i64}]});
    let loaded = Arc::new(catalog(&body));
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &turn_setup(),
        &Value::Null,
        &Value::Null,
        "turns",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    finish_turn(&mut engine);
    assert_eq!(
        engine.query(View::P1, "turn.active").unwrap(),
        Some(json!("P2"))
    );
    let packet = engine.projection(View::P2).unwrap();
    assert_eq!(packet["semantic_state"]["turn_schedule"]["normal"], "P2");
    let mut sampled = Game::from_observation(loaded, &packet, "P2", "sample").unwrap();
    let mut restored: Game =
        serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    for instance in [&mut engine, &mut sampled, &mut restored] {
        for expected in ["P1", "P2", "P1"] {
            finish_turn(instance);
            assert_eq!(
                instance.query(View::P1, "turn.active").unwrap(),
                Some(json!(expected))
            );
        }
    }
}

#[test]
fn skipping_extra_turns_does_not_lose_the_suspended_normal_turn() {
    for (count, expected) in [(1_i64, "P2"), (2_i64, "P1")] {
        let body = json!({"op":"seq","steps":[{"op":"extra_turn","side":"opponent","count":1_i64},{"op":"skip_turn","side":"opponent","count":count}]});
        let mut engine = Game::new(
            Arc::new(catalog(&body)),
            &turn_setup(),
            &Value::Null,
            &Value::Null,
            "turns",
        )
        .unwrap();
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        let events = finish_turn(&mut engine);
        assert_eq!(
            engine.query(View::P1, "turn.active").unwrap(),
            Some(json!(expected))
        );
        let skipped = events
            .iter()
            .filter(|event| event["kind"] == "ターンスキップ")
            .collect::<Vec<_>>();
        assert_eq!(i64::try_from(skipped.len()).unwrap(), count);
        assert_eq!(skipped[0]["extra"], true);
        assert_eq!(
            events
                .iter()
                .filter(|event| event["kind"] == "引く")
                .count(),
            1
        );
        assert_eq!(
            engine.query(View::P1, "turn.elapsed_turns.P2").unwrap(),
            Some(json!(if count == 1 { 3_i64 } else { 2_i64 }))
        );
    }
}

#[test]
fn turn_schedule_import_rejects_invalid_records_and_failed_resolutions_rollback_costs() {
    let baseline_catalog = Arc::new(catalog(&json!({"op":"draw","count":0_i64})));
    for schedule in [
        json!({"skipped":[{"player":"P3","count":1_i64}]}),
        json!({"extra":[{"player":"P1","count":0_i64}]}),
        json!({"extra":[{"player":"P1","count":-1_i64}]}),
        json!({"normal":"P3"}),
        json!({"skipped":[{"player":"P1","count":10_001_i64}]}),
    ] {
        let mut initial = setup();
        initial["semantic_state"] = json!({"turn_schedule":schedule});
        Game::new(
            Arc::clone(&baseline_catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "turns",
        )
        .unwrap_err();
    }
    let mut legacy = setup();
    legacy["turn"]["extra_turns"] = json!(["P1"]);
    assert!(matches!(
        Game::new(
            Arc::clone(&baseline_catalog),
            &legacy,
            &Value::Null,
            &Value::Null,
            "legacy"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    let mut docs =
        document(&json!({"op":"unsupported","reason":"synthetic incomplete resolution"}));
    docs["cards"]["unit-spell"]["abilities"][0]["costs"] =
        json!([{"op":"skip_turn","side":"self","count":1_i64}]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("turns".into(), docs.to_string())],
        )
        .unwrap(),
    );
    let mut engine = Game::new(loaded, &setup(), &Value::Null, &Value::Null, "turns").unwrap();
    let before = engine.digest().unwrap();
    assert!(matches!(
        engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(engine.digest().unwrap(), before);
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "The fixture constructs complete synthetic card maps and validates them before use."
)]
fn effect_evolution_catalog(body: &Value) -> Catalog {
    let mut docs = document(&json!({"op":"draw","count":0_i64}));
    docs["cards"]["unit-follower"]["abilities"] = json!([
        {"kind":"evolve","line":1_i64,"costs":[{"op":"pp","amount":1_i64}],"body":{"op":"evolve","subjects":"self"}},
        {"kind":"activated","line":2_i64,"body":body}
    ]);
    docs["cards"]["unit-evolved"] = json!({"status":"complete","review":"synthetic","abilities":[{"kind":"trigger","line":1_i64,"event":"evolve","subject":"self","body":{"op":"modify","subjects":"self.leader","hp":1_i64}}]});
    docs["cards"]["unit-dual"] = json!({"status":"complete","review":"synthetic","abilities":[]});
    let face = json!({"name":"unit-follower","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"1","power":"3","hp":"5","traits":[],"text":null,"sections":[]});
    let mut alternate = face.clone();
    alternate["name"] = json!("other-evolution");
    alternate["power"] = json!("5");
    alternate["hp"] = json!("4");
    let facts = format!(
        "{}\n{}\n{}",
        snapshot(),
        json!({"number":"unit-evolved","faces":[face]}),
        json!({"number":"unit-dual","faces":[face,alternate]})
    );
    Catalog::from_documents(&facts, &registry(), &[("evolve".into(), docs.to_string())]).unwrap()
}

#[test]
fn effect_evolution_does_not_pay_resources_or_consume_an_evolution_ability_slot() {
    let loaded = Arc::new(effect_evolution_catalog(
        &json!({"op":"evolve","subjects":"self"}),
    ));
    for played in [0_i64, 1_i64] {
        let mut initial = setup();
        initial["turn"]["elapsed_turns"]["P1"] = json!(7_i64);
        initial["players"]["P1"]["ep"] = json!(1_i64);
        initial["players"]["P1"]["sep"] = json!(1_i64);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"a","card":"unit-follower","state":{"power":4_i64,"hp":1_i64,"max_hp":3_i64,"acted":true}},{"id":"c","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"e1","card":"unit-evolved"},{"id":"e2","card":"unit-evolved"}]);
        initial["semantic_state"] = json!({"counters_this_turn":{"P1.evolve_played":played}});
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "effect-evolve",
        )
        .unwrap();
        assert_eq!(
            engine
                .decide(
                    &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
                    "activate"
                )
                .unwrap()
                .outcome,
            "paused"
        );
        assert_eq!(
            engine.legal().unwrap(),
            vec![
                json!({"do":"resolve-choice","select":["e1"]}),
                json!({"do":"resolve-choice","select":["e2"]})
            ]
        );
        let before = engine.digest().unwrap();
        assert_eq!(
            engine
                .decide(&json!({"do":"resolve-choice","select":["b"]}), "wrong")
                .unwrap()
                .outcome,
            "cannot-play"
        );
        assert_eq!(engine.digest().unwrap(), before);
        let mut restored: Game =
            serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
        let mut sampled = Game::from_observation(
            Arc::clone(&loaded),
            &engine.projection(View::P1).unwrap(),
            "P1",
            "sample",
        )
        .unwrap();
        for instance in [&mut engine, &mut restored, &mut sampled] {
            let done = instance
                .decide(&json!({"do":"resolve-choice","select":["e1"]}), "choose")
                .unwrap();
            assert_eq!(done.outcome, "resolved");
            for (path, value) in [
                ("P1.field.a.power", json!(5_i64)),
                ("P1.field.a.hp", json!(3_i64)),
                ("P1.field.a.max_hp", json!(5_i64)),
                ("P1.field.a.acted", json!(true)),
                ("P1.pp.current", json!(2_i64)),
                ("P1.ep", json!(1_i64)),
                ("P1.sep", json!(1_i64)),
                (
                    "semantic_state.counters_this_turn.P1.evolve_played",
                    json!(played),
                ),
                (
                    "semantic_state.counters_this_turn.P1.evolutions",
                    json!(1_i64),
                ),
            ] {
                assert_eq!(instance.query(View::P1, path).unwrap(), Some(value));
            }
            assert!(
                done.events
                    .iter()
                    .any(|event| event["kind"] == "進化" && event["object"] == "a")
            );
            assert!(!done.events.iter().any(|event| event["kind"] == "超進化"));
            instance.decide(&json!({"do":"choose-pending","pending":{"ability":{"source":"a","card":"unit-evolved","line":1_i64}}}), "trigger").unwrap();
            assert_eq!(
                instance.query(View::P1, "P1.leader.life").unwrap(),
                Some(json!(21_i64))
            );
            assert_eq!(
                instance
                    .legal()
                    .unwrap()
                    .iter()
                    .any(|decision| decision["do"] == "evolve" && decision["source"] == "c"),
                played == 0
            );
        }
    }
}

#[test]
fn effect_evolution_selects_named_faces_and_keeps_an_empty_choice_when_unavailable() {
    let loaded = Arc::new(effect_evolution_catalog(
        &json!({"op":"if_done","attempt":{"op":"evolve","subjects":"self","name":"other-evolution"},"then":{"op":"damage","subjects":"opponent.leader","amount":1_i64}}),
    ));
    for unavailable in [false, true] {
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"e","card":"unit-dual","face_up":unavailable}]);
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "effect-evolve",
        )
        .unwrap();
        assert_eq!(
            engine
                .decide(
                    &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
                    "activate"
                )
                .unwrap()
                .outcome,
            "paused"
        );
        let choice = if unavailable {
            json!({"do":"resolve-choice","select":[]})
        } else {
            json!({"do":"resolve-choice","select":["e"],"face":1_i64})
        };
        assert_eq!(engine.legal().unwrap(), vec![choice.clone()]);
        assert_eq!(
            engine.decide(&choice, "choose").unwrap().outcome,
            "resolved"
        );
        assert_eq!(
            engine.query(View::P1, "P1.field.a.power").unwrap(),
            Some(json!(if unavailable { 2_i64 } else { 5_i64 }))
        );
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(if unavailable { 20_i64 } else { 19_i64 }))
        );
    }
}

#[test]
fn already_evolved_cards_ignore_effect_evolution_and_simultaneous_batches_fail_closed() {
    let body = json!({"op":"evolve","subjects":{"zone":"field","side":"opponent"}});
    let loaded = Arc::new(effect_evolution_catalog(&body));
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"b","card":"unit-evolved"}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "effect-evolve",
    )
    .unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
                "activate"
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"b","card":"unit-follower"},{"id":"c","card":"unit-follower"}]);
    let mut unsupported = Game::new(
        loaded,
        &initial,
        &Value::Null,
        &Value::Null,
        "effect-evolve",
    )
    .unwrap();
    let before = unsupported.digest().unwrap();
    assert!(matches!(
        unsupported.decide(
            &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
            "activate"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(unsupported.digest().unwrap(), before);
}

#[test]
fn evolution_cost_layers_are_shared_by_legal_actions_and_payment() {
    let loaded = Arc::new(effect_evolution_catalog(&json!({"op":"seq","steps":[
        {"op":"adjust_cost","subjects":"self","kind":"evolve","amount":2_i64},
        {"op":"adjust_cost","subjects":"self","kind":"evolve","set":0_i64},
        {"op":"adjust_cost","subjects":"self","kind":"play","amount":100_i64}
    ]})));
    let mut initial = setup();
    initial["players"]["P1"]["ep"] = json!(1_i64);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"e","card":"unit-evolved"}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "cost",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
            "adjust",
        )
        .unwrap();
    let options = engine
        .legal()
        .unwrap()
        .into_iter()
        .filter(|choice| choice["do"] == "evolve")
        .collect::<Vec<_>>();
    assert_eq!(options.len(), 2);
    for choice in options {
        assert_eq!(
            choice["pay"]["pp"].as_i64().unwrap() + choice["pay"]["ep"].as_i64().unwrap(),
            2
        );
    }
    let before = engine.digest().unwrap();
    assert_eq!(engine.decide(&json!({"do":"evolve","source":"a","evolve_card":"e","pay":{"pp":0_i64,"ep":0_i64}}), "wrong").unwrap().outcome, "cannot-evolve");
    assert_eq!(engine.digest().unwrap(), before);
    let mut saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let mut sampled =
        Game::from_observation(loaded, &engine.projection(View::P1).unwrap(), "P1", "cost")
            .unwrap();
    for instance in [&mut engine, &mut saved, &mut sampled] {
        assert_eq!(instance.decide(&json!({"do":"evolve","source":"a","evolve_card":"e","pay":{"pp":1_i64,"ep":1_i64}}), "evolve").unwrap().outcome, "resolved");
        assert_eq!(
            instance.query(View::P1, "P1.pp.current").unwrap(),
            Some(json!(1_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P1.ep").unwrap(),
            Some(json!(0_i64))
        );
    }
}

#[test]
fn evolution_discounts_only_consume_the_matching_paid_ability() {
    let loaded = Arc::new(effect_evolution_catalog(&json!({"op":"seq","steps":[
        {"op":"adjust_cost","subjects":{"side":"self","zone":"any"},"kind":"evolve","amount":-1_i64,"uses":1_i64},
        {"op":"adjust_cost","subjects":{"side":"self","zone":"any"},"kind":"play","amount":-1_i64,"uses":1_i64},
        {"op":"evolve","subjects":"self"}
    ]})));
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"a","card":"unit-follower"},{"id":"c","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"e1","card":"unit-evolved"},{"id":"e2","card":"unit-evolved"}]);
    let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "cost").unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
            "adjust",
        )
        .unwrap();
    engine
        .decide(
            &json!({"do":"resolve-choice","select":["e1"]}),
            "effect-evolution",
        )
        .unwrap();
    assert_eq!(
        engine.projection(View::P1).unwrap()["semantic_state"]["continuous_effects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    engine.decide(&json!({"do":"choose-pending","pending":{"ability":{"source":"a","card":"unit-evolved","line":1_i64}}}), "first-trigger").unwrap();
    assert_eq!(engine.decide(&json!({"do":"evolve","source":"c","evolve_card":"e2","pay":{"pp":0_i64,"ep":0_i64}}), "ordinary-evolution").unwrap().outcome, "resolved");
    let remaining =
        engine.projection(View::P1).unwrap()["semantic_state"]["continuous_effects"].clone();
    assert_eq!(remaining.as_array().unwrap().len(), 1);
    assert_eq!(remaining[0]["effect"]["kind"], "play");
    engine.decide(&json!({"do":"choose-pending","pending":{"ability":{"source":"c","card":"unit-evolved","line":1_i64}}}), "second-trigger").unwrap();
    assert_eq!(
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "spell"
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    assert_eq!(
        engine.query(View::P1, "P1.pp.current").unwrap(),
        Some(json!(2_i64))
    );
    assert!(
        engine.projection(View::P1).unwrap()["semantic_state"]["continuous_effects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn counter_aliases_import_to_one_identity_and_roundtrip_through_observations() {
    let mut definitions: Value = serde_json::from_str(&registry()).unwrap();
    definitions["keywords"]["charge"] =
        json!({"ja":"蓄積","aliases":["蓄積カウンター"],"role":"counter"});
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{"kind":"activated","line":1_i64,"body":{"op":"seq","steps":[
        {"op":"counter","subjects":"self","name":"charge","amount":1_i64},
        {"op":"if","condition":{"fn":"ge","args":[{"read":"self.counters.charge"},8_i64]},"then":{"op":"damage","subjects":"opponent.leader","amount":1_i64}}
    ]}}]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &definitions.to_string(),
            &[("aliases".into(), doc.to_string())],
        )
        .unwrap(),
    );
    for name in ["charge", "蓄積", "蓄積カウンター"] {
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["field"][0]["state"]["counters"][name] = json!(7_i64);
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "aliases",
        )
        .unwrap();
        let first = engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                "tick",
            )
            .unwrap();
        assert!(
            first
                .events
                .iter()
                .any(|event| event["kind"] == "カウンター"
                    && event["name"] == "蓄積"
                    && event["delta"] == 1_i64)
        );
        assert_eq!(
            engine.query(View::P1, "P1.field.a.counters").unwrap(),
            Some(json!({"蓄積":8_i64}))
        );
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(19_i64))
        );
        let mut saved: Game =
            serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
        let mut sampled = Game::from_observation(
            Arc::clone(&loaded),
            &engine.projection(View::P1).unwrap(),
            "P1",
            "aliases",
        )
        .unwrap();
        for instance in [&mut engine, &mut saved, &mut sampled] {
            instance
                .decide(
                    &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                    "tick-again",
                )
                .unwrap();
            assert_eq!(
                instance.query(View::P1, "P1.field.a.counters").unwrap(),
                Some(json!({"蓄積":9_i64}))
            );
            assert_eq!(
                instance.query(View::P1, "P2.leader.life").unwrap(),
                Some(json!(18_i64))
            );
        }
    }
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["field"][0]["state"]["counters"] =
        json!({"charge":7_i64,"蓄積":7_i64});
    assert!(matches!(
        Game::new(loaded, &initial, &Value::Null, &Value::Null, "duplicate"),
        Err(EngineFailure::Invalid(_))
    ));
}

#[test]
fn keyword_aliases_reject_ambiguous_ids_labels_and_malformed_lists() {
    for aliases in [
        json!(["guard"]),
        json!(["守護"]),
        json!(["蓄積", "蓄積"]),
        json!([1_i64]),
        json!([""]),
        json!("蓄積"),
    ] {
        let mut definitions: Value = serde_json::from_str(&registry()).unwrap();
        definitions["keywords"]["charge"] = json!({"ja":"蓄積","aliases":aliases,"role":"counter"});
        Catalog::from_documents(&snapshot(), &definitions.to_string(), &[]).unwrap_err();
    }
    let mut definitions: Value = serde_json::from_str(&registry()).unwrap();
    definitions["keywords"]["charge"] = json!({"ja":"蓄積","aliases":["共通"],"role":"counter"});
    definitions["keywords"]["different"] = json!({"ja":"別物","aliases":["共通"],"role":"counter"});
    Catalog::from_documents(&snapshot(), &definitions.to_string(), &[]).unwrap_err();
}

#[expect(
    clippy::indexing_slicing,
    reason = "The fixture builds known synthetic card and ability maps."
)]
fn stat_event_document(body: &Value) -> Value {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([
        {"kind":"trigger","line":1_i64,"event":"hp_increase","subject":"self","body":{"op":"modify","subjects":"self.leader","hp":{"fn":"sub","args":[{"read":"event.after"},{"read":"event.before"}]}}},
        {"kind":"activated","line":2_i64,"body":body}
    ]);
    doc
}

#[test]
fn stat_increase_triggers_compare_actual_values_and_collect_after_the_whole_batch() {
    for (change, expected) in [
        (json!({"hp":2_i64}), true),
        (json!({"set_hp":5_i64}), true),
        (json!({"hp":0_i64}), false),
        (json!({"set_hp":3_i64}), false),
        (json!({"hp":-1_i64}), false),
    ] {
        let mut body = json!({"op":"modify","subjects":{"side":"self","zone":"field"}});
        body.as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        let mut doc = stat_event_document(&body);
        doc["cards"]["unit-follower"]["abilities"][0]["trigger_if"] = json!({"fn":"eq","args":[{"count":{"side":"self","zone":"field","where":{"fn":"ge","args":[{"read":"item.hp"},5_i64]}}},2_i64]});
        let loaded = Arc::new(
            Catalog::from_documents(
                &snapshot(),
                &registry(),
                &[("stats".into(), doc.to_string())],
            )
            .unwrap(),
        );
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"a","card":"unit-follower"},{"id":"c","card":"unit-follower"}]);
        let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "stats").unwrap();
        let step = engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
                "modify",
            )
            .unwrap();
        let waiting = step
            .events
            .iter()
            .filter(|event| event["kind"] == "待機")
            .collect::<Vec<_>>();
        assert_eq!(waiting.len(), if expected { 2 } else { 0 });
        if expected {
            assert_eq!(waiting[0]["group"], waiting[1]["group"]);
            for source in ["a", "c"] {
                engine.decide(&json!({"do":"choose-pending","pending":{"ability":{"source":source,"line":1_i64}}}), source).unwrap();
            }
        }
        assert_eq!(
            engine.query(View::P1, "P1.leader.life").unwrap(),
            Some(json!(if expected { 24_i64 } else { 20_i64 }))
        );
        assert_eq!(
            engine
                .query(View::P1, "P1.field.a.stats_increased_this_turn")
                .unwrap(),
            Some(json!(expected))
        );
    }
}

#[test]
fn repeated_stat_events_keep_distinct_contexts_and_identical_trigger_copies() {
    let mut doc = stat_event_document(&json!({"op":"seq","steps":[
        {"op":"modify","subjects":"self","hp":1_i64},
        {"op":"modify","subjects":"self","set_hp":8_i64}
    ]}));
    doc["cards"]["unit-follower"]["abilities"].as_array_mut().unwrap().push(json!({"kind":"static","line":3_i64,"body":{"op":"repeat_triggers","side":"self","event":"hp_increase","additional":1_i64}}));
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("stats".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &setup(),
        &Value::Null,
        &Value::Null,
        "stats",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
            "modify",
        )
        .unwrap();
    let pending =
        engine.projection(View::P1).unwrap()["semantic_state"]["pending_triggers"].clone();
    assert_eq!(pending.as_array().unwrap().len(), 4);
    assert_eq!(pending[0]["id"], pending[1]["id"]);
    assert_eq!(pending[2]["id"], pending[3]["id"]);
    assert_ne!(pending[0]["id"], pending[2]["id"]);
    let legal = engine.legal().unwrap();
    assert_eq!(legal.len(), 4);
    let before = engine.digest().unwrap();
    assert!(matches!(
        engine.decide(
            &json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1_i64}}}),
            "ambiguous"
        ),
        Err(EngineFailure::Invalid(_))
    ));
    assert_eq!(engine.digest().unwrap(), before);
    let mut saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let mut sampled =
        Game::from_observation(loaded, &engine.projection(View::P1).unwrap(), "P1", "stats")
            .unwrap();
    for instance in [&mut engine, &mut saved, &mut sampled] {
        for (index, life) in [(2, 24_i64), (3, 28_i64), (0, 29_i64), (1, 30_i64)] {
            instance.decide(&legal[index], "trigger").unwrap();
            assert_eq!(
                instance.query(View::P1, "P1.leader.life").unwrap(),
                Some(json!(life))
            );
        }
    }
}

#[test]
fn evolution_collects_stat_triggers_from_the_new_face() {
    let mut doc = stat_event_document(&json!({"op":"evolve","subjects":"self"}));
    doc["cards"]["unit-evolved"] = json!({"status":"complete","review":"synthetic","abilities":[{"kind":"trigger","line":1_i64,"event":"hp_increase","subject":"self","body":{"op":"modify","subjects":"self.leader","hp":1_i64}}]});
    let evolved = json!({"number":"unit-evolved","faces":[{"name":"unit-follower","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"1","power":"4","hp":"5","traits":[],"text":null,"sections":[]}]});
    let facts = format!("{}\n{evolved}", snapshot());
    let loaded = Arc::new(
        Catalog::from_documents(&facts, &registry(), &[("stats".into(), doc.to_string())]).unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"e","card":"unit-evolved"}]);
    let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "stats").unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
            "activate",
        )
        .unwrap();
    engine
        .decide(&json!({"do":"resolve-choice","select":["e"]}), "evolve")
        .unwrap();
    assert_eq!(
        engine.legal().unwrap(),
        vec![
            json!({"do":"choose-pending","pending":{"ability":{"source":"a","card":"unit-evolved","line":1_i64}}})
        ]
    );
    engine.decide(&json!({"do":"choose-pending","pending":{"ability":{"source":"a","card":"unit-evolved","line":1_i64}}}),"trigger").unwrap();
    assert_eq!(
        engine.query(View::P1, "P1.leader.life").unwrap(),
        Some(json!(21_i64))
    );
}

#[test]
fn empty_stack_waits_for_resolution_and_can_be_refilled_before_check_timing() {
    for refill in [false, true] {
        let loaded = Arc::new(stack_catalog(&json!({"op":"seq","steps":[
            {"op":"move","subjects":{"side":"self","zone":"field","type":"amulet"},"to":"cemetery"},
            {"op":"optional","then":{"op":"seq","steps":[
                {"op":"counter","subjects":{"side":"self","zone":"field","type":"amulet"},"name":"stack_counter","amount":i64::from(refill)},
                {"op":"damage","subjects":"target.1","amount":1_i64}
            ]}}
        ]})));
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":"soil","card":"unit-soil","state":{"counters":{"stack_counter":1_i64}}}));
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "stack-check",
        )
        .unwrap();
        assert_eq!(
            engine
                .decide(
                    &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                    "cast"
                )
                .unwrap()
                .outcome,
            "paused"
        );
        assert_eq!(
            engine
                .query(View::P1, "P1.field.soil.counters.スタックカウンター")
                .unwrap(),
            Some(json!(0_i64))
        );
        let mut saved: Game =
            serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
        let mut sampled = Game::from_observation(
            loaded,
            &engine.projection(View::P1).unwrap(),
            "P1",
            "stack-check",
        )
        .unwrap();
        for instance in [&mut engine, &mut saved, &mut sampled] {
            let step = instance
                .decide(&json!({"do":"resolve-choice","choice":"execute"}), "finish")
                .unwrap();
            assert_eq!(step.outcome, "resolved");
            assert_eq!(
                instance.query(View::P1, "P1.field").unwrap(),
                Some(if refill {
                    json!(["a", "soil"])
                } else {
                    json!(["a"])
                })
            );
            assert!(
                !step
                    .events
                    .iter()
                    .any(|event| event["kind"] == "破壊" && event["object"] == "soil")
            );
            let cleanup = step.events.iter().position(|event| {
                event["kind"] == "移動" && event["object"] == "soil" && event["by"] == "rule-11.7.1"
            });
            assert_eq!(cleanup.is_some(), !refill);
            if let Some(index) = cleanup {
                let damage = step
                    .events
                    .iter()
                    .position(|event| event["kind"] == "ダメージ")
                    .unwrap();
                let erased = step
                    .events
                    .iter()
                    .position(|event| event["kind"] == "消去" && event["object"] == "soil")
                    .unwrap();
                assert!(damage < index && index < erased);
            }
        }
    }
}

#[test]
fn empty_stacks_and_lethal_followers_leave_in_one_state_based_batch() {
    let loaded = Arc::new(stack_catalog(&json!({"op":"seq","steps":[
        {"op":"counter","subjects":{"side":"self","zone":"field","type":"amulet"},"name":"stack_counter","amount":-1_i64},
        {"op":"damage","subjects":"target.1","amount":3_i64}
    ]})));
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().extend([
        json!({"id":"soil-1","card":"unit-soil","state":{"counters":{"stack_counter":1_i64}}}),
        json!({"id":"soil-2","card":"unit-soil","state":{"counters":{"stack_counter":1_i64}}}),
        json!({"id":"silenced","card":"unit-soil","state":{"silenced":true,"counters":{"stack_counter":1_i64}}})
    ]);
    let mut engine =
        Game::new(loaded, &initial, &Value::Null, &Value::Null, "stack-check").unwrap();
    let step = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let movements = step
        .events
        .iter()
        .filter(|event| {
            event["kind"] == "移動"
                && ["soil-1", "soil-2", "b"]
                    .iter()
                    .any(|id| event["object"] == *id)
        })
        .collect::<Vec<_>>();
    assert_eq!(movements.len(), 3);
    assert!(
        movements
            .iter()
            .all(|event| event["group"] == movements[0]["group"])
    );
    assert_eq!(
        engine.query(View::P1, "P1.field").unwrap(),
        Some(json!(["a", "silenced"]))
    );
    assert_eq!(
        engine.query(View::P1, "P2.cemetery").unwrap(),
        Some(json!(["b"]))
    );
    assert_eq!(
        step.events
            .iter()
            .filter(|event| event["kind"] == "破壊")
            .count(),
        1
    );
}

#[test]
fn nonprogressing_rule_movement_fails_closed_instead_of_recursing_forever() {
    let mut doc = document(&json!({"op":"damage","subjects":"target.1","amount":3_i64}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{"kind":"static","line":1_i64,"body":{"op":"replace_move","subjects":"self","from":"field","to":"cemetery","replacement":"field"}}]);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("rule-loop".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut engine = Game::new(loaded, &setup(), &Value::Null, &Value::Null, "rule-loop").unwrap();
    let before = engine.digest().unwrap();
    assert!(matches!(
        engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(engine.digest().unwrap(), before);
}

#[test]
fn deck_reordering_preserves_order_identity_state_and_only_counts_changed_positions() {
    for (position, chosen, expected, changed) in [
        (
            json!("top"),
            json!(["d2", "d0"]),
            json!(["d2", "d0", "d1", "tail"]),
            2_i64,
        ),
        (
            json!("bottom"),
            json!(["d2", "d0"]),
            json!(["d1", "tail", "d2", "d0"]),
            1_i64,
        ),
        (
            json!(2_i64),
            json!(["d2", "d0"]),
            json!(["d1", "d2", "d0", "tail"]),
            2_i64,
        ),
        (
            json!("top"),
            json!(["d0", "d1"]),
            json!(["d0", "d1", "d2", "tail"]),
            0_i64,
        ),
    ] {
        let body = json!({"op":"seq","steps":[
            {"op":"reveal","subjects":{"side":"self","zone":"deck"},"to":"all"},
            {"op":"select","select":{"side":"self","zone":"deck"},"min":2_i64,"max":2_i64,"order":true,"bind":"chosen"},
            {"op":"move","subjects":"chosen","to":"deck","position":position,"bind":"moved"},
            {"op":"damage","subjects":"opponent.leader","amount":{"count":"moved"}},
            {"op":"modify","subjects":"chosen","power":1_i64}
        ]});
        let loaded = Arc::new(catalog(&body));
        let mut initial = setup();
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"d0","card":"unit-follower","state":{"power":8_i64,"counters":{"memory":2_i64}}},
            {"id":"d1","card":"unit-follower"},{"id":"d2","card":"unit-spell"},{"id":"tail","card":"unit-spell"}
        ]);
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &initial,
            &Value::Null,
            &Value::Null,
            "order",
        )
        .unwrap();
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        let mut saved: Game =
            serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
        let mut sampled =
            Game::from_observation(loaded, &engine.projection(View::P1).unwrap(), "P1", "order")
                .unwrap();
        for instance in [&mut engine, &mut saved, &mut sampled] {
            let step = instance
                .decide(&json!({"do":"resolve-choice","order":chosen}), "place")
                .unwrap();
            assert_eq!(step.outcome, "resolved");
            assert_eq!(
                instance.query(View::P1, "P1.deck").unwrap(),
                Some(expected.clone())
            );
            for (path, value) in [
                ("P1.deck.d0.generation", json!(0_i64)),
                ("P1.deck.d0.power", json!(9_i64)),
                ("P1.deck.d0.counters.memory", json!(2_i64)),
                ("P2.leader.life", json!(20_i64 - changed)),
            ] {
                assert_eq!(instance.query(View::P1, path).unwrap(), Some(value));
            }
            let opponent = instance.projection(View::P2).unwrap();
            for id in chosen
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
            {
                assert!(
                    !opponent["P1"]["deck"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(id))
                );
                assert!(opponent["objects"].get(id).is_none());
            }
        }
    }
}

#[test]
fn ordered_deck_insertion_splits_anonymous_runs_at_card_offsets() {
    let body = json!({"op":"seq","steps":[
        {"op":"select","select":{"side":"self","zone":"cemetery"},"min":2_i64,"max":2_i64,"order":true,"bind":"chosen"},
        {"op":"move","subjects":"chosen","to":"deck","position":3_i64}
    ]});
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"filler":5_i64},{"id":"tail","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"c1","card":"unit-follower","state":{"power":8_i64}},{"id":"c2","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::new(catalog(&body)),
        &initial,
        &Value::Null,
        &Value::Null,
        "order",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let mut saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    for instance in [&mut engine, &mut saved] {
        instance
            .decide(&json!({"do":"resolve-choice","order":["c2","c1"]}), "place")
            .unwrap();
        assert_eq!(
            instance.query(View::P1, "P1.deck").unwrap(),
            Some(json!([{"filler":2_i64},"c2","c1",{"filler":3_i64},{"filler":1_i64}]))
        );
        assert_eq!(
            instance.query(View::P1, "P1.deck_count").unwrap(),
            Some(json!(8_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P1.deck.c1.generation").unwrap(),
            Some(json!(1_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P1.deck.c1.power").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(
            instance.query(View::P2, "P1.deck").unwrap(),
            Some(
                json!([{"filler":2_i64},{"filler":1_i64},{"filler":1_i64},{"filler":3_i64},{"filler":1_i64}])
            )
        );
    }
}

#[test]
fn ordered_batches_keep_independent_offsets_for_each_destination_deck() {
    let body = json!({"op":"seq","steps":[
        {"op":"select","select":{"side":"both","zone":"cemetery"},"min":4_i64,"max":4_i64,"order":true,"bind":"chosen"},
        {"op":"move","subjects":"chosen","to":"deck","position":1_i64}
    ]});
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"t1","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"t2","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"c1","card":"unit-follower"},{"id":"c2","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["cemetery"] =
        json!([{"id":"x1","card":"unit-follower"},{"id":"x2","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::new(catalog(&body)),
        &initial,
        &Value::Null,
        &Value::Null,
        "order",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    engine
        .decide(
            &json!({"do":"resolve-choice","order":["x2","c2","x1","c1"]}),
            "place",
        )
        .unwrap();
    let private: Value = serde_json::to_value(&engine).unwrap();
    assert_eq!(
        private["state"]["players"]["P1"]["zones"]["deck"],
        json!(["c2", "c1", "t1"])
    );
    assert_eq!(
        private["state"]["players"]["P2"]["zones"]["deck"],
        json!(["x2", "x1", "t2"])
    );
    assert_eq!(
        engine.query(View::P1, "P2.deck").unwrap(),
        Some(json!(["x2","x1",{"filler":1_i64}]))
    );
    let owner = engine.projection(View::P2).unwrap();
    assert!(owner["objects"].get("x1").is_none());
    assert!(owner["objects"].get("x2").is_none());
}

#[test]
fn explicit_ordering_in_unordered_zones_is_rejected_transactionally() {
    let mut engine =
        game(&json!({"op":"move","subjects":"target.1","to":"cemetery","position":"top"}));
    let before = engine.digest().unwrap();
    assert!(matches!(
        engine.decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(engine.digest().unwrap(), before);
}

#[test]
fn full_control_destination_keeps_empty_input_and_resumes_later_transfers() {
    let body = json!({"op":"seq","steps":[
        {"op":"if_done","attempt":{"op":"control","subjects":"target.1","side":"self"},"then":{"op":"damage","subjects":"opponent.leader","amount":3_i64}},
        {"op":"control","subjects":"a","side":"opponent"}
    ]});
    let loaded = Arc::new(stack_catalog(&body));
    let mut initial = setup();
    for index in 0_u32..4_u32 {
        initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":format!("soil-{index}"),"card":"unit-soil","state":{"counters":{"stack_counter":1_i64}}}));
    }
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "control",
    )
    .unwrap();
    let step = engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(step.outcome, "paused");
    assert_eq!(
        engine.projection(View::P1).unwrap()["awaiting"]["choices"],
        json!([{"do":"resolve-choice","select":[]}])
    );
    let mut saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let mut sampled = Game::from_observation(
        loaded,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "control",
    )
    .unwrap();
    for instance in [&mut engine, &mut saved, &mut sampled] {
        let before = instance.digest().unwrap();
        assert_eq!(
            instance
                .decide(&json!({"do":"resolve-choice","select":["b"]}), "overfull")
                .unwrap()
                .outcome,
            "cannot-play"
        );
        assert_eq!(instance.digest().unwrap(), before);
        let finished = instance
            .decide(&json!({"do":"resolve-choice","select":[]}), "skip")
            .unwrap();
        assert_eq!(finished.outcome, "resolved");
        for (path, value) in [
            ("P2.leader.life", json!(20_i64)),
            ("P1.field_count", json!(4_i64)),
            ("P2.field", json!(["b", "a"])),
            ("P2.field.a.generation", json!(0_i64)),
        ] {
            assert_eq!(instance.query(View::P1, path).unwrap(), Some(value));
        }
        assert!(
            !finished
                .events
                .iter()
                .any(|event| event["kind"] == "移動" && event["object"] == "b")
        );
    }
}

#[test]
fn control_batches_select_capacity_preserve_state_and_share_a_movement_group() {
    let body = json!({"op":"if_done","attempt":{"op":"control","subjects":{"zone":"field","side":"opponent","type":"follower"},"side":"self"},"then":{"op":"damage","subjects":"opponent.leader","amount":2_i64}});
    for available in [1_i64, 2_i64] {
        let loaded = Arc::new(stack_catalog(&body));
        let mut initial = setup();
        for index in 0_i64..4_i64 - available {
            initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":format!("soil-{index}"),"card":"unit-soil","state":{"counters":{"stack_counter":1_i64}}}));
        }
        initial["players"]["P2"]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":"b2","card":"unit-follower","state":{"power":7_i64,"hp":1_i64,"acted":true,"counters":{"memory":2_i64}}}));
        let mut engine =
            Game::new(loaded, &initial, &Value::Null, &Value::Null, "control").unwrap();
        let mut step = engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        if available == 1 {
            assert_eq!(step.outcome, "paused");
            assert_eq!(
                engine.projection(View::P1).unwrap()["awaiting"]["choices"],
                json!([{"do":"resolve-choice","select":["b"]},{"do":"resolve-choice","select":["b2"]}])
            );
            step = engine
                .decide(&json!({"do":"resolve-choice","select":["b2"]}), "choose")
                .unwrap();
        }
        assert_eq!(step.outcome, "resolved");
        for (path, value) in [
            ("P1.field_count", json!(5_i64)),
            ("P2.leader.life", json!(18_i64)),
            ("P1.field.b2.generation", json!(0_i64)),
            ("P1.field.b2.power", json!(7_i64)),
            ("P1.field.b2.hp", json!(1_i64)),
            ("P1.field.b2.acted", json!(true)),
            ("P1.field.b2.counters.memory", json!(2_i64)),
        ] {
            assert_eq!(engine.query(View::P1, path).unwrap(), Some(value));
        }
        let moved = step
            .events
            .iter()
            .filter(|event| event["kind"] == "移動" && event["to"] == "P1.field")
            .collect::<Vec<_>>();
        assert_eq!(i64::try_from(moved.len()).unwrap(), available);
        assert!(
            moved
                .iter()
                .all(|event| event["group"] == moved[0]["group"])
        );
        assert!(!step.events.iter().any(|event| event["kind"] == "場に出す"));
        assert!(
            engine.projection(View::P1).unwrap()["known_cards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|card| card["id"] == "b2" && card["owner"] == "P2")
        );
    }
}

#[test]
fn granted_trigger_and_activation_use_the_recipient_and_saved_provider_after_source_leaves() {
    let body = json!({"op":"modify","subjects":"target.1","abilities":[
        {"line":1_i64,"kind":"trigger","event":"main_start","body":{"op":"damage","subjects":"self.leader","amount":2_i64}},
        {"line":1_i64,"kind":"activated","costs":[{"op":"pp","amount":1_i64}],"body":{"op":"move","subjects":"self","to":"cemetery"}}
    ]});
    let loaded = Arc::new(catalog(&body));
    let mut initial = setup();
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"d","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "grant",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P1.cemetery").unwrap(),
        Some(json!(["s"]))
    );
    finish_turn(&mut engine);
    let reference = json!({"source":"b","card":"unit-spell","line":1_i64});
    assert_eq!(
        engine.projection(View::P2).unwrap()["awaiting"]["choices"],
        json!([{"do":"choose-pending","pending":{"ability":reference}}])
    );
    let mut saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let mut sampled =
        Game::from_observation(loaded, &engine.projection(View::P2).unwrap(), "P2", "grant")
            .unwrap();
    for instance in [&mut engine, &mut saved, &mut sampled] {
        instance
            .decide(
                &json!({"do":"choose-pending","pending":{"ability":reference}}),
                "trigger",
            )
            .unwrap();
        assert_eq!(
            instance.query(View::P2, "P2.leader.life").unwrap(),
            Some(json!(18_i64))
        );
        assert!(
            instance
                .legal()
                .unwrap()
                .contains(&json!({"do":"activate","ability":reference}))
        );
        instance
            .decide(&json!({"do":"activate","ability":reference}), "activate")
            .unwrap();
        for (path, value) in [
            ("P2.cemetery", json!(["b"])),
            ("P2.pp.current", json!(2_i64)),
            ("P1.pp.current", json!(1_i64)),
        ] {
            assert_eq!(instance.query(View::P2, path).unwrap(), Some(value));
        }
        assert_eq!(
            instance
                .decide(&json!({"do":"activate","ability":reference}), "lost")
                .unwrap()
                .outcome,
            "cannot-activate"
        );
    }
}

#[test]
fn losing_abilities_erases_prior_grants_but_allows_later_grants_and_expiry_restores_them() {
    let early = json!({"line":1_i64,"kind":"activated","body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}});
    let late = json!({"line":2_i64,"kind":"activated","body":{"op":"damage","subjects":"opponent.leader","amount":3_i64}});
    let body = json!({"op":"seq","steps":[
        {"op":"modify","subjects":"a","abilities":[early]},
        {"op":"modify","subjects":"a","remove_abilities":true,"until":"end-of-turn"},
        {"op":"modify","subjects":"a","abilities":[late]}
    ]});
    let loaded = Arc::new(catalog(&body));
    let mut engine = Game::new(loaded, &turn_setup(), &Value::Null, &Value::Null, "grant").unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let early_action =
        json!({"do":"activate","ability":{"source":"a","card":"unit-spell","line":1_i64}});
    let late_action =
        json!({"do":"activate","ability":{"source":"a","card":"unit-spell","line":2_i64}});
    assert!(!engine.legal().unwrap().contains(&early_action));
    assert!(engine.legal().unwrap().contains(&late_action));
    let before = engine.digest().unwrap();
    assert_eq!(
        engine.decide(&early_action, "erased").unwrap().outcome,
        "cannot-activate"
    );
    assert_eq!(engine.digest().unwrap(), before);
    engine.decide(&late_action, "late").unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(17_i64))
    );
    finish_turn(&mut engine);
    finish_turn(&mut engine);
    for action in [&early_action, &late_action] {
        assert!(engine.legal().unwrap().contains(action));
    }
    engine.decide(&early_action, "restored").unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.leader.life").unwrap(),
        Some(json!(16_i64))
    );
}

#[test]
fn temporary_grants_expire_and_amulet_guard_does_not_restrict_attack_targets() {
    let body = json!({"op":"seq","steps":[
        {"op":"become_type","subjects":"target.1","type":"amulet"},
        {"op":"modify","subjects":"target.1","abilities":[{"line":1_i64,"kind":"static","body":{"op":"keyword","name":"guard"}}],"until":"end-of-turn"}
    ]});
    let loaded = Arc::new(catalog(&body));
    let mut initial = turn_setup();
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":false});
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "grant").unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P2.field.b.keywords").unwrap(),
        Some(json!(["守護"]))
    );
    assert!(
        engine
            .legal()
            .unwrap()
            .contains(&json!({"do":"attack","attacker":"a","target":"P2.leader"}))
    );
    finish_turn(&mut engine);
    assert_eq!(
        engine.query(View::P2, "P2.field.b.keywords").unwrap(),
        Some(json!([]))
    );
}

#[test]
fn ambiguous_or_unsupported_grants_fail_closed_without_partial_payment() {
    for grant in [
        json!({"op":"modify","subjects":"a","abilities":[{"line":1_i64,"kind":"activated","limit":1_i64,"body":{"op":"draw","count":1_i64}}]}),
        json!({"op":"modify","subjects":"a","until":"end-of-turn","power":1_i64,"abilities":[]}),
        json!({"op":"modify","subjects":"a","abilities":[{"line":1_i64,"kind":"activated","active_zones":["hand"],"body":{"op":"draw","count":1_i64}}]}),
    ] {
        let mut engine = game(&grant);
        let before = engine.digest().unwrap();
        assert!(matches!(
            engine.decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "unsupported"
            ),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(engine.digest().unwrap(), before);
    }
    let mut engine = game(&json!({"op":"modify","subjects":"a","abilities":[
        {"line":1_i64,"kind":"activated","body":{"op":"damage","subjects":"opponent.leader","amount":1_i64}},
        {"line":1_i64,"kind":"activated","body":{"op":"damage","subjects":"opponent.leader","amount":2_i64}}
    ]}));
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let before = engine.digest().unwrap();
    assert!(matches!(
        engine.decide(
            &json!({"do":"activate","ability":{"source":"a","card":"unit-spell","line":1_i64}}),
            "ambiguous"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(engine.digest().unwrap(), before);
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "The fixture defines all authored ability and setup fields before indexing."
)]
fn attack_requirement_fixture(prohibited: bool) -> (Arc<Catalog>, Value) {
    let requirement = json!({"line":1_i64,"kind":"static","body":{"op":"require_attack","subjects":{"side":"opponent","zone":"field","type":"follower"},"count":1_i64,"condition":{"read":"self.acted"}}});
    let body = json!({"op":"seq","steps":[
        {"op":"act","subjects":"a"},
        {"op":"modify","subjects":"a","during":"next-opponent-turn","abilities":[requirement.clone(),requirement]}
    ]});
    let mut doc = document(&body);
    doc["cards"]["unit-follower"]["abilities"] = json!([
        {"line":1_i64,"kind":"static","body":{"op":"keyword","name":"storm"}},
        {"line":2_i64,"kind":"activated","body":{"op":"stand","subjects":"self"}},
        {"line":3_i64,"kind":"activated","body":{"op":"act","subjects":"self"}},
        {"line":4_i64,"kind":"activated","body":{"op":"move","subjects":"self","to":"hand"}}
    ]);
    if prohibited {
        doc["cards"]["unit-follower"]["abilities"].as_array_mut().unwrap().push(json!({"line":5_i64,"kind":"static","body":{"op":"restrict","subjects":"self","action":"attack"}}));
    }
    let mut keywords: Value = serde_json::from_str(&registry()).unwrap();
    keywords["keywords"]["storm"] =
        json!({"ja":"疾走","expansion":{"op":"keyword","name":"storm"}});
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &keywords.to_string(),
            &[("obligation".into(), doc.to_string())],
        )
        .unwrap(),
    );
    (loaded, turn_setup())
}

#[test]
fn attack_requirements_follow_the_future_period_and_current_object_generation() {
    let (loaded, initial) = attack_requirement_fixture(false);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "obligation",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "grant",
        )
        .unwrap();
    assert!(engine.legal().unwrap().contains(&json!({"do":"end-phase"})));
    finish_turn(&mut engine);
    assert!(!engine.legal().unwrap().contains(&json!({"do":"end-phase"})));
    let before = engine.digest().unwrap();
    assert_eq!(
        engine
            .decide(&json!({"do":"end-phase"}), "early-end")
            .unwrap()
            .outcome,
        "cannot-play"
    );
    assert_eq!(engine.digest().unwrap(), before);
    engine
        .decide(
            &json!({"do":"attack","attacker":"b","target":"P1.leader"}),
            "attack",
        )
        .unwrap();
    engine.decide(&json!({"do":"pass"}), "quick-pass").unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"b","line":2_i64}}),
            "stand",
        )
        .unwrap();
    assert!(engine.legal().unwrap().contains(&json!({"do":"end-phase"})));
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"b","line":4_i64}}),
            "bounce",
        )
        .unwrap();
    engine
        .decide(&json!({"do":"play","card":"b"}), "replay")
        .unwrap();
    assert_eq!(
        engine
            .query(View::P2, "P2.field.b.attacks_this_turn")
            .unwrap(),
        Some(json!(0_i64))
    );
    let counts =
        engine.projection(View::P2).unwrap()["semantic_state"]["counters_this_turn"].clone();
    assert_eq!(counts["b.attacks"], 0_i64);
    assert_eq!(counts["P2.attacks"], 1_i64);
    assert!(!engine.legal().unwrap().contains(&json!({"do":"end-phase"})));
    let mut saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let mut sampled = Game::from_observation(
        loaded,
        &engine.projection(View::P2).unwrap(),
        "P2",
        "obligation",
    )
    .unwrap();
    for instance in [&mut engine, &mut saved, &mut sampled] {
        assert!(
            !instance
                .legal()
                .unwrap()
                .contains(&json!({"do":"end-phase"}))
        );
        instance
            .decide(
                &json!({"do":"activate","ability":{"source":"b","line":3_i64}}),
                "act-instead",
            )
            .unwrap();
        assert!(
            instance
                .legal()
                .unwrap()
                .contains(&json!({"do":"end-phase"}))
        );
        finish_turn(instance);
        finish_turn(instance);
        assert!(
            instance
                .legal()
                .unwrap()
                .contains(&json!({"do":"end-phase"}))
        );
    }
}

#[test]
fn attack_requirements_never_override_prohibitions_or_an_inactive_source_condition() {
    for (prohibited, stand_source) in [(true, false), (false, true)] {
        let (loaded, initial) = attack_requirement_fixture(prohibited);
        let mut engine =
            Game::new(loaded, &initial, &Value::Null, &Value::Null, "obligation").unwrap();
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "grant",
            )
            .unwrap();
        if stand_source {
            engine
                .decide(
                    &json!({"do":"activate","ability":{"source":"a","line":2_i64}}),
                    "stand-source",
                )
                .unwrap();
        }
        finish_turn(&mut engine);
        assert!(engine.legal().unwrap().contains(&json!({"do":"end-phase"})));
        assert_eq!(
            engine
                .legal()
                .unwrap()
                .iter()
                .any(|action| action["do"] == "attack"),
            !prohibited
        );
    }
}

#[test]
fn unknown_future_grant_periods_and_mixed_numeric_windows_rollback() {
    for body in [
        json!({"op":"modify","subjects":"target.1","during":"next-moonrise","abilities":[]}),
        json!({"op":"modify","subjects":"target.1","during":"next-opponent-turn","power":1_i64,"abilities":[]}),
        json!({"op":"modify","subjects":"target.1","during":"next-opponent-turn","power":1_i64}),
    ] {
        let mut engine = game(&body);
        let before = engine.digest().unwrap();
        assert!(matches!(
            engine.decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "unknown-period"
            ),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(engine.digest().unwrap(), before);
    }
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "The resource fixture constructs its authored programs and complete player zones."
)]
fn ride_fixture(extra_costs: &[Value]) -> (Arc<Catalog>, Value) {
    let resource = json!({"number":"unit-resource","faces":[{"name":"ドライブポイント","card_class":"ニュートラル","card_type":"スペル・エボルヴ","cost":"0","power":"-","hp":"-","traits":[],"text":null,"sections":[]}]});
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    let mut costs = vec![json!({"op":"pp","amount":1_i64})];
    costs.extend_from_slice(extra_costs);
    doc["cards"]["unit-follower"]["abilities"] = json!([{"line":1_i64,"kind":"ride","costs":costs,"body":{"op":"gain_drive","subjects":"self"}}]);
    doc["cards"]["unit-resource"] =
        json!({"status":"complete","review":"synthetic","abilities":[]});
    let loaded = Arc::new(
        Catalog::from_documents(
            &format!("{}\n{resource}", snapshot()),
            &drive_registry(),
            &[("ride".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["ep"] = json!(1_i64);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"r1","card":"unit-resource"},
        {"id":"r2","card":"unit-resource"},
        {"id":"used","card":"unit-resource","face_up":true}
    ]);
    (loaded, initial)
}

#[test]
fn explicit_and_implicit_ride_costs_choose_and_pay_exactly_one_resource() {
    let link = json!({"op":"link_resource","subjects":"self","name":"ドライブポイント","count":1_i64,"from_zone":"evolve_deck","to":"drive"});
    for explicit in [false, true] {
        for ep in [0_i64, 1_i64] {
            let (loaded, initial) = ride_fixture(if explicit { from_ref(&link) } else { &[] });
            let mut engine = Game::new(
                Arc::clone(&loaded),
                &initial,
                &Value::Null,
                &Value::Null,
                "ride",
            )
            .unwrap();
            let choices = engine
                .legal()
                .unwrap()
                .into_iter()
                .filter(|action| action["do"] == "activate")
                .collect::<Vec<_>>();
            assert_eq!(choices.len(), 4);
            assert!(
                choices
                    .iter()
                    .all(|action| action["costs"].as_object().unwrap().len() == 1
                        && action["costs"]["1"].as_array().unwrap().len() == 1)
            );
            let mut saved: Game =
                serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
            let mut sampled =
                Game::from_observation(loaded, &engine.projection(View::P1).unwrap(), "P1", "ride")
                    .unwrap();
            for instance in [&mut engine, &mut saved, &mut sampled] {
                let before = instance.digest().unwrap();
                for material in [json!([]), json!(["used"]), json!(["r1", "r2"])] {
                    assert_eq!(instance.decide(&json!({"do":"activate","ability":{"source":"a","line":1_i64},"costs":{"1":material},"pay":{"pp":1_i64-ep,"ep":ep}}),"bad-resource").unwrap().outcome,"cannot-activate");
                    assert_eq!(instance.digest().unwrap(), before);
                }
                instance.decide(&json!({"do":"activate","ability":{"source":"a","line":1_i64},"costs":{"1":["r2"]},"pay":{"pp":1_i64-ep,"ep":ep}}),"ride").unwrap();
                for (path, value) in [
                    ("P1.drive", json!(["r2"])),
                    ("P1.evolve_deck", json!(["r1", "used"])),
                    ("P1.field.a.links.憑依", json!(["r2"])),
                    ("P1.pp.current", json!(1_i64 + ep)),
                    ("P1.ep", json!(1_i64 - ep)),
                ] {
                    assert_eq!(instance.query(View::P1, path).unwrap(), Some(value));
                }
                assert!(
                    !instance
                        .legal()
                        .unwrap()
                        .iter()
                        .any(|action| action["do"] == "activate")
                );
            }
        }
    }
}

#[test]
fn ride_payment_rejects_ambiguous_declarations_and_missing_unique_materials() {
    let link = json!({"op":"link_resource","subjects":"self","name":"ドライブポイント","count":1_i64,"from_zone":"evolve_deck","to":"drive"});
    let mut doubled = link.clone();
    doubled["count"] = json!(2_i64);
    for costs in [vec![link.clone(), link.clone()], vec![doubled]] {
        let (loaded, initial) = ride_fixture(&costs);
        let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "ride").unwrap();
        let before = engine.digest().unwrap();
        assert!(matches!(engine.decide(&json!({"do":"activate","ability":{"source":"a","line":1_i64},"pay":{"pp":1_i64,"ep":0_i64}}),"unknown-cost"),Err(EngineFailure::Unsupported(_))));
        assert_eq!(engine.digest().unwrap(), before);
    }
    let (loaded, mut initial) = ride_fixture(&[link]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"r1","card":"unit-resource"}]);
    let mut engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "ride").unwrap();
    let before = engine.digest().unwrap();
    assert_eq!(engine.decide(&json!({"do":"activate","ability":{"source":"a","line":1_i64},"pay":{"pp":1_i64,"ep":0_i64}}),"missing").unwrap().outcome,"cannot-activate");
    assert_eq!(engine.digest().unwrap(), before);
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "The fixture extends valid registry JSON with the three rule keywords."
)]
fn drive_registry() -> String {
    let mut data: Value = serde_json::from_str(&registry()).unwrap();
    for (id, name) in [
        ("drive", "ドライブ"),
        ("single_drive", "シングルドライブ"),
        ("rush", "突進"),
    ] {
        data["keywords"][id] = json!({"ja":name,"expansion":{"op":"keyword","name":id}});
    }
    data.to_string()
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "The fixture builds complete synthetic abilities, target declarations and zones."
)]
fn drive_fixture() -> (Arc<Catalog>, Value) {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    let mut abilities = vec![
        json!({"line":1_i64,"kind":"activated","body":{"op":"gain_drive","subjects":"self"}}),
        json!({"line":2_i64,"kind":"trigger","event":"gain_drive","subject":"self","body":{"op":"modify","subjects":"self","power":1_i64}}),
    ];
    for (line, body) in [
        (3_i64, json!({"op":"gain_drive","subjects":"target.1"})),
        (
            4_i64,
            json!({"op":"modify","subjects":"target.1","remove_abilities":true}),
        ),
        (
            5_i64,
            json!({"op":"control","subjects":"target.1","side":"self"}),
        ),
        (
            6_i64,
            json!({"op":"seq","steps":[
                {"op":"move","subjects":"target.1","to":"hand","bind":"returned"},
                {"op":"move","subjects":"returned","to":"field","bind":"entered"},
                {"op":"gain_drive","subjects":"entered"}
            ]}),
        ),
    ] {
        abilities.push(json!({"line":line,"kind":"activated","targets":[{"key":"1","select":{"side":"both","zone":"field","type":"follower"},"min":1_i64,"max":1_i64}],"body":body}));
    }
    doc["cards"]["unit-follower"]["abilities"] = json!(abilities);
    let loaded = Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &drive_registry(),
            &[("drive".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["field"][0]["state"]["entered_this_turn"] = json!(true);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(true);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"top","card":"unit-spell"},{"id":"bottom","card":"unit-spell"}]);
    (loaded, initial)
}

#[test]
fn gained_drive_installs_rule_abilities_and_triggers_only_once_without_a_resource_link() {
    let (loaded, initial) = drive_fixture();
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "drive",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
            "gain",
        )
        .unwrap();
    engine
        .decide(
            &json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":2_i64}}}),
            "boost",
        )
        .unwrap();
    let mut saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let mut sampled =
        Game::from_observation(loaded, &engine.projection(View::P1).unwrap(), "P1", "drive")
            .unwrap();
    for instance in [&mut engine, &mut saved, &mut sampled] {
        assert_eq!(
            instance.query(View::P1, "P1.field.a.power").unwrap(),
            Some(json!(3_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P1.field.a.keywords").unwrap(),
            Some(json!(["ドライブ", "突進", "シングルドライブ"]))
        );
        instance
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                "gain-again",
            )
            .unwrap();
        assert_eq!(
            instance
                .query(View::P1, "semantic_state.pending_triggers")
                .unwrap(),
            Some(json!([]))
        );
        assert!(
            !instance
                .legal()
                .unwrap()
                .iter()
                .any(|action| action["do"] == "attack" && action["target"] == "P2.leader")
        );
        instance
            .decide(
                &json!({"do":"attack","attacker":"a","target":"b"}),
                "attack",
            )
            .unwrap();
        let pending = instance.legal().unwrap();
        assert_eq!(
            pending,
            vec![
                json!({"do":"choose-pending","pending":{"ability":{"source":"a","rule":"14.4.7.3.2","keyword":"シングルドライブ"}}})
            ]
        );
        let before = instance.query(View::Referee, "P1.deck").unwrap().unwrap();
        let mut after = before.as_array().unwrap().clone();
        after.rotate_left(1);
        instance.decide(&pending[0], "check").unwrap();
        assert_eq!(
            instance.query(View::Referee, "P1.deck").unwrap(),
            Some(json!(after))
        );
        assert_eq!(
            instance.query(View::P1, "P1.trigger").unwrap(),
            Some(json!([]))
        );
    }
}

#[test]
fn drive_history_survives_silence_and_control_but_a_new_generation_can_gain_again() {
    let (loaded, initial) = drive_fixture();
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "drive",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":3_i64},"targets":{"1":["b"]}}),
            "gain",
        )
        .unwrap();
    engine
        .decide(
            &json!({"do":"choose-pending","pending":{"ability":{"source":"b","line":2_i64}}}),
            "boost",
        )
        .unwrap();
    for line in [4_i64, 5_i64] {
        engine.decide(&json!({"do":"activate","ability":{"source":"a","line":line},"targets":{"1":["b"]}}),"silence-control").unwrap();
    }
    let mut saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let mut sampled =
        Game::from_observation(loaded, &engine.projection(View::P1).unwrap(), "P1", "drive")
            .unwrap();
    for instance in [&mut engine, &mut saved, &mut sampled] {
        instance.decide(&json!({"do":"activate","ability":{"source":"a","line":3_i64},"targets":{"1":["b"]}}),"no-regain").unwrap();
        assert_eq!(
            instance.query(View::P1, "P1.field.b.keywords").unwrap(),
            Some(json!([]))
        );
        assert_eq!(
            instance.query(View::P1, "P1.field.b.drive_gained").unwrap(),
            Some(json!(true))
        );
        assert_eq!(
            instance
                .query(View::P1, "semantic_state.pending_triggers")
                .unwrap(),
            Some(json!([]))
        );
        instance.decide(&json!({"do":"activate","ability":{"source":"a","line":6_i64},"targets":{"1":["b"]}}),"return-regain").unwrap();
        assert_eq!(
            instance.query(View::P1, "P2.field.b.generation").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(
            instance.query(View::P1, "P2.field.b.keywords").unwrap(),
            Some(json!(["ドライブ", "突進", "シングルドライブ"]))
        );
        instance
            .decide(
                &json!({"do":"choose-pending","pending":{"ability":{"source":"b","line":2_i64}}}),
                "new-boost",
            )
            .unwrap();
        assert_eq!(
            instance.query(View::P1, "P2.field.b.power").unwrap(),
            Some(json!(3_i64))
        );
    }
}

#[test]
fn a_resource_link_alone_never_grants_drive_and_existing_drive_does_not_gain_again() {
    for gained in [false, true] {
        let (loaded, mut initial) = drive_fixture();
        initial["players"]["P1"]["zones"]["field"][0]["state"]["links"] = json!({"憑依":[]});
        if gained {
            initial["players"]["P1"]["zones"]["field"][0]["state"]["keywords"] =
                json!(["ドライブ"]);
        }
        let mut engine =
            Game::new(loaded, &initial, &Value::Null, &Value::Null, "prior-drive").unwrap();
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                "gain",
            )
            .unwrap();
        let pending = engine
            .query(View::P1, "semantic_state.pending_triggers")
            .unwrap()
            .unwrap();
        assert_eq!(pending.as_array().unwrap().is_empty(), gained);
    }
    let (loaded, mut initial) = ride_fixture(&[]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([]);
    initial["players"]["P1"]["zones"]["drive"] = json!([{"id":"r1","card":"unit-resource"}]);
    initial["players"]["P1"]["zones"]["field"][0]["state"]["links"] = json!({"憑依":["r1"]});
    let engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "cost-only").unwrap();
    assert_eq!(
        engine.query(View::P1, "P1.field.a.keywords").unwrap(),
        Some(json!([]))
    );
}

#[test]
fn observed_ownership_constrains_each_public_deck_list_after_control_changes() {
    let (loaded, mut initial) = drive_fixture();
    initial["room"] = json!({"open_decklists":true});
    initial["players"]["P1"]["deck_list"] =
        json!([{"card":"unit-follower","count":1_i64},{"card":"unit-spell","count":3_i64}]);
    initial["players"]["P2"]["deck_list"] = json!([{"card":"unit-follower","count":1_i64}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["keywords"] = json!(["ドライブ"]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "owner",
    )
    .unwrap();
    for line in [4_i64, 5_i64, 3_i64] {
        engine.decide(&json!({"do":"activate","ability":{"source":"a","line":line},"targets":{"1":["b"]}}),"silence-control-regain").unwrap();
    }
    assert_eq!(
        engine.query(View::P1, "P1.field.b.keywords").unwrap(),
        Some(json!([]))
    );
    assert_eq!(
        engine
            .query(View::P1, "semantic_state.pending_triggers")
            .unwrap(),
        Some(json!([]))
    );
    for (view, seat) in [(View::P1, "P1"), (View::P2, "P2")] {
        let mut sampled = Game::from_observation(
            Arc::clone(&loaded),
            &engine.projection(view).unwrap(),
            seat,
            "owner",
        )
        .unwrap();
        let packet = sampled.projection(view).unwrap();
        assert!(
            packet["known_cards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|known| known["id"] == "b" && known["owner"] == "P2")
        );
        sampled.decide(&json!({"do":"activate","ability":{"source":"a","line":6_i64},"targets":{"1":["b"]}}),"return").unwrap();
        assert_eq!(sampled.query(view, "P2.field").unwrap(), Some(json!(["b"])));
        assert_eq!(
            sampled
                .query(view, "semantic_state.pending_triggers")
                .unwrap()
                .unwrap()[0]["controller"],
            json!("P2")
        );
    }
    initial["players"]["P1"]["zones"]["field"][0]["owner"] = json!("unknown");
    Game::new(loaded, &initial, &Value::Null, &Value::Null, "bad-owner").unwrap_err();
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "The history fixture provides literal printed fragments and matching authored nodes."
)]
fn numeric_history_fixture(buff: &Value, text: &str) -> (Arc<Catalog>, Value) {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    let mut lines = vec![snapshot()];
    for (number, printed, body) in [
        ("unit-buff", text, buff.clone()),
        (
            "unit-set",
            "それの攻撃力と体力を1にする。",
            json!({"op":"modify","subjects":"target.1","set_power":1_i64,"set_hp":1_i64}),
        ),
    ] {
        lines.push(json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"スペル","cost":"0","power":"-","hp":"-","traits":[],"text":printed,"sections":[]}]}).to_string());
        doc["cards"][number] = json!({"status":"complete","review":"synthetic","abilities":[{"line":1_i64,"kind":"spell","body":body}]});
    }
    let loaded = Arc::new(
        Catalog::from_documents(
            &lines.join("\n"),
            &registry(),
            &[("numeric-history".into(), doc.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"buff","card":"unit-buff"},{"id":"set","card":"unit-set"}]);
    initial["semantic_state"]["continuous_effects"] = json!([
        {"id":"later","source":"set","text":"これの攻撃力と体力を1にする。","applies_to":["a"],"until":"permanent","order":2_i64},
        {"id":"earlier","source":"buff","text":text.replace("それは", "これは").replace(['{','}'],""),"applies_to":["a"],"until":"permanent","order":1_i64}
    ]);
    (loaded, initial)
}

#[test]
fn literal_numeric_history_uses_timestamps_once_and_preserves_explicit_effective_values() {
    for reverse in [false, true] {
        for effective in [false, true] {
            let (loaded, mut initial) = numeric_history_fixture(
                &json!({"op":"modify","subjects":"target.1","power":2_i64}),
                "それは{攻撃力}+2する。",
            );
            if reverse {
                initial["semantic_state"]["continuous_effects"][0]["order"] = json!(1_i64);
                initial["semantic_state"]["continuous_effects"][1]["order"] = json!(2_i64);
            }
            let power = if reverse { 3_i64 } else { 1_i64 };
            if effective {
                initial["players"]["P1"]["zones"]["field"][0]["state"] =
                    json!({"power":power,"hp":1_i64});
            }
            let mut engine = Game::new(
                Arc::clone(&loaded),
                &initial,
                &Value::Null,
                &Value::Null,
                "history",
            )
            .unwrap();
            let mut saved: Game =
                serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
            let mut sampled = Game::from_observation(
                loaded,
                &engine.projection(View::P1).unwrap(),
                "P1",
                "history",
            )
            .unwrap();
            for instance in [&mut engine, &mut saved, &mut sampled] {
                for (path, expected) in [
                    ("P1.field.a.power", json!(power)),
                    ("P1.field.a.hp", json!(1_i64)),
                    ("P1.field.a.max_hp", json!(1_i64)),
                    ("P1.field.a.stats_increased_this_turn", json!(false)),
                    ("semantic_state.pending_triggers", json!([])),
                ] {
                    assert_eq!(instance.query(View::P1, path).unwrap(), Some(expected));
                }
                instance
                    .decide(
                        &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
                        "attack",
                    )
                    .unwrap();
                instance.decide(&json!({"do":"pass"}), "combat").unwrap();
                assert_eq!(
                    instance.query(View::P1, "P2.leader.life").unwrap(),
                    Some(json!(20_i64 - power))
                );
            }
        }
    }
}

#[test]
fn historical_health_modifiers_reconstruct_maximum_health_without_double_counting_damage() {
    for explicit in [json!({"damage":3_i64}), json!({"hp":2_i64})] {
        let (loaded, mut initial) = numeric_history_fixture(
            &json!({"op":"modify","subjects":"target.1","hp":2_i64}),
            "それは{体力}+2する。",
        );
        let buff = initial["semantic_state"]["continuous_effects"][1].clone();
        initial["semantic_state"]["continuous_effects"] = json!([buff]);
        initial["players"]["P1"]["zones"]["field"][0]["state"] = explicit;
        let engine = Game::new(loaded, &initial, &Value::Null, &Value::Null, "hp-history").unwrap();
        for (field, expected) in [("hp", 2_i64), ("max_hp", 5_i64), ("damage", 3_i64)] {
            assert_eq!(
                engine
                    .query(View::P1, &format!("P1.field.a.{field}"))
                    .unwrap(),
                Some(json!(expected))
            );
        }
    }
}

#[test]
fn unresolved_numeric_history_never_silently_uses_printed_defaults() {
    for node in [
        json!({"op":"modify","subjects":"target.1","power":{"read":"x"}}),
        json!({"op":"modify","subjects":"target.1","power":2_i64,"keywords":["guard"]}),
        json!({"op":"seq","steps":[{"op":"modify","subjects":"self","power":2_i64},{"op":"modify","subjects":"target.1","power":2_i64}]}),
    ] {
        let (loaded, initial) = numeric_history_fixture(&node, "それは{攻撃力}+2する。");
        assert!(matches!(
            Game::new(loaded, &initial, &Value::Null, &Value::Null, "bad-history"),
            Err(EngineFailure::Unsupported(_))
        ));
    }
    for (key, value) in [
        ("until", json!("end-of-turn")),
        ("order", Value::Null),
        ("text", json!("")),
        ("text", json!("別の能力")),
        ("order", json!(2_i64)),
    ] {
        let (loaded, mut initial) = numeric_history_fixture(
            &json!({"op":"modify","subjects":"target.1","power":2_i64}),
            "それは{攻撃力}+2する。",
        );
        initial["semantic_state"]["continuous_effects"][1][key] = value;
        assert!(matches!(
            Game::new(loaded, &initial, &Value::Null, &Value::Null, "bad-history"),
            Err(EngineFailure::Unsupported(_))
        ));
    }
}

#[test]
fn visible_names_follow_the_current_face_and_rule_name_without_exposing_hidden_cards() {
    let mut doc = document(&json!({"op":"draw","count":0_i64}));
    let mut rows: Vec<Value> = snapshot()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut second_face = rows[0]["faces"][0].clone();
    second_face["name"] = json!("second-face-name");
    rows[0]["faces"].as_array_mut().unwrap().push(second_face);
    doc["cards"]["unit-spell"]["rules_name"] = json!("private-rule-name");
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let loaded = Arc::new(
        Catalog::from_documents(&snapshot, &registry(), &[("names".into(), doc.to_string())])
            .unwrap(),
    );
    let mut initial = setup();
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"face":1_i64});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"secret","card":"unit-spell"}]);
    let engine = Game::new(
        Arc::clone(&loaded),
        &initial,
        &Value::Null,
        &Value::Null,
        "names",
    )
    .unwrap();
    assert_eq!(
        engine.query(View::P1, "P1.field.a.name").unwrap(),
        Some(json!("second-face-name"))
    );
    assert_eq!(
        engine.query(View::P2, "P2.hand.secret.name").unwrap(),
        Some(json!("private-rule-name"))
    );
    assert_eq!(engine.query(View::P1, "P2.hand.secret.name").unwrap(), None);
    let packet = engine.projection(View::P1).unwrap();
    assert!(!packet.to_string().contains("private-rule-name"));
    assert!(!packet.to_string().contains("secret"));
    let restored = Game::from_observation(loaded, &packet, "P1", "names").unwrap();
    assert_eq!(
        restored.query(View::P1, "P1.field.a.name").unwrap(),
        Some(json!("second-face-name"))
    );
}

#[test]
fn named_creation_omits_prints_but_explicit_capacity_selections_retain_them() {
    for zone in ["field", "ex"] {
        for capacity_choice in [false, true] {
            let body = json!({"op":"create","name":"shared-token","count":if capacity_choice {5_i64} else {1_i64},"to":zone});
            let (facts, doc) = token_fixture(&body);
            let loaded = Arc::new(
                Catalog::from_documents(&facts, &registry(), &[("names".into(), doc.to_string())])
                    .unwrap(),
            );
            let mut initial = setup();
            if zone == "ex" {
                initial["players"]["P1"]["zones"]["ex"] =
                    json!([{"id":"occupied","card":"unit-follower"}]);
            }
            let mut engine =
                Game::new(loaded, &initial, &Value::Null, &Value::Null, "named-create").unwrap();
            let mut step = engine
                .decide(
                    &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                    "create",
                )
                .unwrap();
            if capacity_choice {
                assert_eq!(step.outcome, "paused");
                let choice = engine.legal().unwrap().remove(0);
                step = engine.decide(&choice, "prints").unwrap();
            }
            let events: Vec<_> = step
                .events
                .iter()
                .filter(|event| event["from"] == "P1.void")
                .collect();
            assert_eq!(events.len(), if capacity_choice { 4 } else { 1 });
            assert!(
                events
                    .iter()
                    .all(|event| event.get("card").is_some() == capacity_choice)
            );
            assert_eq!(
                engine
                    .query(View::P1, &format!("P1.{zone}.new-1.name"))
                    .unwrap(),
                Some(json!("shared-token"))
            );
        }
    }
}

#[test]
fn both_start_amulet_choices_are_known_only_to_the_chooser_and_shuffle_hides_leftovers() {
    for scripted in [false, true] {
        let mut position = json!({"pregame":true,"players":{}});
        let mut scripts = Vec::new();
        for seat in ["P1", "P2"] {
            let mut deck = vec![
                json!({"id":format!("{seat}-start"),"card":"unit-follower"}),
                json!({"id":format!("{seat}-leftover"),"card":"unit-follower"}),
            ];
            let mut order = vec![json!(format!("{seat}-leftover"))];
            for index in 0_u8..8 {
                let id = format!("{seat}-card-{index}");
                deck.push(json!({"id":id,"card":"unit-spell"}));
                order.push(json!(id));
            }
            position["players"][seat] = json!({"construction":"title","title":"カードファイト!! ヴァンガード","leader":{"class":"ニュートラル"},"deck_list":deck,"evolve_deck_list":[]});
            scripts.push(json!({"player":seat,"zone":"deck","result":order}));
        }
        let random = if scripted {
            json!({"first_chooser":"P1","shuffles":scripts})
        } else {
            Value::Null
        };
        let mut engine =
            Game::new(opening_catalog(), &position, &Value::Null, &random, "both").unwrap();
        let before = engine.projection(View::P2).unwrap();
        assert_eq!(before["objects"]["P2-start"]["card"], "unit-follower");
        assert_eq!(
            before["knowledge"]["identifiable"],
            json!(["P2-leftover", "P2-start"])
        );
        let chosen = engine
            .decide(
                &json!({"do":"choose-start-amulet","object":"P1-start"}),
                "first-amulet",
            )
            .unwrap();
        assert_eq!(chosen.events.len(), 1);
        assert_eq!(chosen.events[0]["kind"], "場に出す");
        assert_eq!(chosen.events[0]["by"], "rule-14.4.3.1");
        assert_eq!(chosen.events[0]["face_up"], false);
        let private = engine.projection(View::P2).unwrap();
        assert!(!private.to_string().contains("P1-start"));
        assert!(!private.to_string().contains("P1-leftover"));
        assert_eq!(private["P1"]["field_count"], 1_i64);
        assert_eq!(private["objects"]["P2-start"]["card"], "unit-follower");
        let mut restored: Game =
            serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
        restored
            .decide(
                &json!({"do":"choose-start-amulet","object":"P2-start"}),
                "second-amulet",
            )
            .unwrap();
        for (view, seat, other) in [(View::P1, "P1", "P2"), (View::P2, "P2", "P1")] {
            let packet = restored.projection(view).unwrap();
            assert_eq!(packet["objects"][format!("{seat}-start")]["face_up"], false);
            assert_eq!(
                packet["objects"][format!("{seat}-start")]["generation"],
                1_i64
            );
            assert!(packet["objects"].get(format!("{seat}-leftover")).is_none());
            assert_eq!(
                packet["knowledge"]["located"],
                json!([format!("{seat}-start")])
            );
            assert!(!packet.to_string().contains(&format!("{other}-start")));
        }
        restored
            .decide(&json!({"do":"choose-first","first":"P1"}), "first")
            .unwrap();
        restored
            .decide(&json!({"do":"mulligan","redo":false}), "keep-one")
            .unwrap();
        restored
            .decide(&json!({"do":"mulligan","redo":false}), "keep-two")
            .unwrap();
        for view in [View::P1, View::P2] {
            let packet = restored.projection(view).unwrap();
            assert_eq!(packet["objects"]["P1-start"]["face_up"], true);
            assert_eq!(packet["objects"]["P2-start"]["face_up"], true);
            assert_eq!(packet["objects"]["P1-start"]["generation"], 1_i64);
        }
    }
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "Synthetic source sentences distinguish provenance from executable declarations."
)]
fn suppression_fixture(local: bool, enabled: bool, sentence: &str) -> (Arc<Catalog>, Game) {
    let body = json!({"op":"optional","then":{"op":"move","subjects":{"side":"self","zone":"hand","type":"follower"},"to":"field","suppress_fanfare":local}});
    let mut doc = document(&body);
    doc["cards"]["unit-spell"]["abilities"][0]["targets"] = json!([]);
    doc["cards"]["unit-spell"]["abilities"][0]["section"] = json!(0_i64);
    doc["cards"]["unit-follower"]["abilities"] = json!([{ "kind":"trigger","line":1_i64,"event":"enter","subject":"self","limit":1_i64,"limit_at":"trigger","body":{"op":"modify","subjects":"self.leader","hp":1_i64}}]);
    doc["cards"]["unit-observer"] = json!({"status":"complete","review":"synthetic","abilities":[{"kind":"trigger","line":1_i64,"event":"enter","subject":{"zone":"field","side":"self","other":true},"body":{"op":"modify","subjects":"self.leader","hp":1_i64}}]});
    doc["cards"]["unit-blocker"] = json!({"status":"complete","review":"synthetic","abilities":[{"kind":"static","line":1_i64,"section":0_i64,"body":{"op":"restrict","action":"trigger","subjects":"opponent.leader","events":["fanfare","on_evolve"],"condition":enabled}}]});
    let mut cards = snapshot()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    cards[1]["faces"][0]["text"] = json!("別の文。");
    cards[1]["faces"][0]["sections"] = json!([format!("選んで場に出す。{sentence}後の文。")]);
    let mut observer = cards[0].clone();
    observer["number"] = json!("unit-observer");
    let mut blocker = cards[0].clone();
    blocker["number"] = json!("unit-blocker");
    blocker["faces"][0]["text"] = json!("別の文。");
    blocker["faces"][0]["sections"] = json!([format!("前の文。{sentence}後の文。")]);
    cards.extend([observer, blocker]);
    let catalog = Catalog::from_documents(
        &cards
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        &registry(),
        &[("suppression.yaml".into(), doc.to_string())],
    )
    .unwrap();
    let mut position = setup();
    position["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-observer"}]);
    position["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"s","card":"unit-spell"},{"id":"entrant","card":"unit-follower"}]);
    position["players"]["P2"]["zones"]["field"] = if local {
        json!([])
    } else {
        json!([{"id":"blocker","card":"unit-blocker"}])
    };
    let shared = Arc::new(catalog);
    let game = Game::new(
        Arc::clone(&shared),
        &position,
        &Value::Null,
        &Value::Null,
        "suppress",
    )
    .unwrap();
    (shared, game)
}

#[test]
fn suppression_precedes_trigger_limits_and_preserves_unrelated_entry_triggers() {
    for (local, enabled) in [(true, true), (false, true), (false, false)] {
        let reason = if local {
            "それの{ファンファーレ}能力は誘発しない。"
        } else {
            "これが場にいる限り、相手プレイヤーすべての{ファンファーレ}能力と【進化時】能力は誘発しない。"
        };
        let (catalog, mut engine) = suppression_fixture(local, enabled, reason);
        assert_eq!(
            engine
                .decide(&json!({"do":"play","card":"s"}), "spell")
                .unwrap()
                .outcome,
            "paused"
        );
        let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
        let sampled = Game::from_observation(
            catalog,
            &engine.projection(View::P1).unwrap(),
            "P1",
            "suppression",
        )
        .unwrap();
        for mut restored in [saved, sampled] {
            let result = restored
                .decide(&json!({"do":"resolve-choice","choice":"execute"}), "entry")
                .unwrap();
            let suppressed = result
                .events
                .iter()
                .filter(|event| event["kind"] == "抑制")
                .collect::<Vec<_>>();
            let packet = restored.projection(View::P1).unwrap();
            let pending = packet["semantic_state"]["pending_triggers"]
                .as_array()
                .unwrap();
            assert!(
                pending
                    .iter()
                    .any(|entry| entry["ability"]["source"] == "observer")
            );
            if local || enabled {
                assert_eq!(suppressed.len(), 1);
                assert_eq!(suppressed[0]["reason"], reason);
                assert_eq!(suppressed[0]["ability"]["source"], "entrant");
                assert_eq!(pending.len(), 1);
                assert_eq!(packet["semantic_state"]["used_this_turn"], json!([]));
            } else {
                assert!(suppressed.is_empty());
                assert_eq!(pending.len(), 2);
                assert_eq!(
                    packet["semantic_state"]["used_this_turn"][0]["count"],
                    1_i64
                );
            }
        }
    }
}

#[test]
fn unknown_or_ambiguous_suppression_provenance_rolls_back_the_resolution() {
    for reason in [
        "無関係な文。",
        "{ファンファーレ}能力は誘発しない。別の{ファンファーレ}能力は誘発しない。",
    ] {
        let (_, mut engine) = suppression_fixture(true, true, reason);
        engine
            .decide(&json!({"do":"play","card":"s"}), "spell")
            .unwrap();
        let before = engine.digest().unwrap();
        assert!(matches!(
            engine.decide(&json!({"do":"resolve-choice","choice":"execute"}), "entry"),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(engine.digest().unwrap(), before);
    }
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "Synthetic reprints isolate crest name uniqueness from printed identities."
)]
fn crest_catalog(body: &Value) -> Arc<Catalog> {
    let mut doc = document(body);
    doc["cards"]["unit-spell"]["abilities"][0]["targets"] = json!([]);
    let mut printed = snapshot();
    for (number, name, kind) in [
        ("crest-first", "shared-crest", "クレスト・トークン"),
        ("crest-reprint", "shared-crest", "クレスト・トークン"),
        ("crest-other", "other-crest", "クレスト・トークン"),
        ("ordinary-token", "ordinary", "フォロワー・トークン"),
    ] {
        printed.push('\n');
        printed.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"0","power":"1","hp":"1","traits":[],"text":null,"sections":[]}]}).to_string());
        doc["cards"][number] = json!({"status":"complete","review":"synthetic","abilities":[]});
    }
    doc["cards"]["crest-first"]["token_template"] = json!(true);
    Arc::new(
        Catalog::from_documents(
            &printed,
            &registry(),
            &[("crests.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    )
}

#[test]
fn crest_reprints_and_simultaneous_copies_are_filtered_before_capacity_or_identity_allocation() {
    let body = json!({"op":"if_done","attempt":{"op":"create","name":"shared-crest","count":2_i64,"to":"ex"},"then":{"op":"modify","subjects":"self.leader","hp":3_i64}});
    let mut position = setup();
    position["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"existing","card":"crest-reprint"},{"filler":4_i64}]);
    let mut engine = Game::new(
        crest_catalog(&body),
        &position,
        &Value::Null,
        &Value::Null,
        "full-crest",
    )
    .unwrap();
    let result = engine
        .decide(&json!({"do":"play","card":"s"}), "duplicate")
        .unwrap();
    assert_eq!(result.outcome, "resolved");
    assert!(result.events.iter().all(|event| {
        !event["object"]
            .as_str()
            .is_some_and(|id| id.starts_with("new-"))
    }));
    assert_eq!(
        engine.query(View::P1, "P1.ex_count").unwrap(),
        Some(json!(5_i64))
    );
    assert_eq!(
        engine.query(View::P1, "P1.leader.life").unwrap(),
        Some(json!(20_i64))
    );

    let batch = json!({"op":"create","to":"ex","tokens":[
        {"name":"shared-crest","count":2_i64},
        {"name":"other-crest","count":2_i64},
        {"name":"ordinary","count":2_i64}
    ]});
    let loaded = crest_catalog(&batch);
    position["players"]["P1"]["zones"]["ex"] = json!([]);
    let mut batch_engine = Game::new(
        Arc::clone(&loaded),
        &position,
        &Value::Null,
        &Value::Null,
        "batch-crests",
    )
    .unwrap();
    assert_eq!(
        batch_engine
            .decide(&json!({"do":"play","card":"s"}), "batch")
            .unwrap()
            .outcome,
        "resolved"
    );
    let packet = batch_engine.projection(View::P1).unwrap();
    assert_eq!(
        packet["P1"]["ex"],
        json!(["new-1", "new-2", "new-3", "new-4"])
    );
    assert_eq!(packet["objects"]["new-1"]["name"], "shared-crest");
    assert_eq!(packet["objects"]["new-2"]["name"], "other-crest");
    assert_eq!(packet["objects"]["new-3"]["name"], "ordinary");
    assert_eq!(packet["objects"]["new-4"]["name"], "ordinary");
    let rebuilt = Game::from_observation(loaded, &packet, "P1", "crests").unwrap();
    assert_eq!(
        rebuilt.projection(View::P1).unwrap()["P1"]["ex"],
        packet["P1"]["ex"]
    );
}

#[test]
fn advances_return_face_up_to_the_owner_before_the_effect_continues() {
    for (origin, destination) in [
        ("field", "hand"),
        ("ex", "deck"),
        ("hand", "cemetery"),
        ("field", "banish"),
        ("field", "ex"),
        ("ex", "field"),
    ] {
        let owner = if origin == "hand" { "P2" } else { "P1" };
        let owner_side = if owner == "P1" { "self" } else { "opponent" };
        let body = json!({"op":"seq","steps":[
            {"op":"move","subjects":{"side":"opponent","zone":origin},"to":destination},
            {"op":"modify","subjects":"self.leader","hp":{"count":{"side":owner_side,"zone":"evolve_deck"}}}
        ]});
        let mut doc = document(&body);
        doc["cards"]["unit-spell"]["abilities"][0]["targets"] = json!([]);
        doc["cards"]["unit-advance"] =
            json!({"status":"complete","review":"synthetic","abilities":[]});
        let printed = json!({"number":"unit-advance","faces":[{"name":"advance","card_class":"ニュートラル","card_type":"フォロワー・アドバンス","cost":"1","power":"3","hp":"3","traits":[],"text":null,"sections":[]}]});
        let loaded = Arc::new(
            Catalog::from_documents(
                &format!("{}\n{printed}", snapshot()),
                &registry(),
                &[("advance.yaml".into(), doc.to_string())],
            )
            .unwrap(),
        );
        let mut position = setup();
        position["players"]["P2"]["zones"] = json!({origin:[{"id":"b","card":"unit-advance","owner":owner,"state":{"silenced":true,"power":9_i64}}]});
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &position,
            &Value::Null,
            &Value::Null,
            "advance",
        )
        .unwrap();
        let result = engine
            .decide(&json!({"do":"play","card":"s"}), "move")
            .unwrap();
        assert_eq!(result.outcome, "resolved");
        let returns = result
            .events
            .iter()
            .filter(|event| event["by"] == "rule-9.2.2")
            .collect::<Vec<_>>();
        let should_return = !matches!(destination, "field" | "ex");
        let packet = engine.projection(View::P1).unwrap();
        if should_return {
            assert_eq!(returns.len(), 1);
            assert_eq!(returns[0]["from"], format!("{owner}.{destination}"));
            assert_eq!(returns[0]["to"], format!("{owner}.evolve_deck"));
            assert!(
                !packet[owner][destination]
                    .as_array()
                    .unwrap()
                    .contains(&json!("b"))
            );
            assert_eq!(packet[owner]["evolve_deck"], json!(["b"]));
            assert_eq!(packet["objects"]["b"]["face_up"], true);
            assert_eq!(packet["objects"]["b"]["generation"], 2_i64);
            assert_eq!(packet["P1"]["leader"]["life"], 21_i64);
            assert_eq!(
                engine
                    .query(View::P2, &format!("{owner}.evolve_deck.b.face_up"))
                    .unwrap(),
                Some(json!(true))
            );
            let rebuilt = Game::from_observation(loaded, &packet, "P1", "advance").unwrap();
            assert_eq!(
                rebuilt
                    .query(View::P1, &format!("{owner}.evolve_deck.b.face_up"))
                    .unwrap(),
                Some(json!(true))
            );
        } else {
            assert!(returns.is_empty());
            assert_eq!(packet["P2"][destination], json!(["b"]));
            assert_eq!(packet["P1"]["leader"]["life"], 20_i64);
        }
    }
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "Two synthetic evolution faces deliberately have distinct names and capabilities."
)]
fn dual_evolution_catalog() -> Arc<Catalog> {
    let mut doc = document(&json!({"op":"seq","steps":[]}));
    doc["cards"]["unit-follower"]["abilities"] = json!([{ "kind":"evolve","line":1_i64,"costs":[{"op":"pp","amount":1_i64}],"body":{"op":"evolve","subjects":"self","names":["bright","dark"]}}]);
    doc["cards"]["unit-dual"] = json!({"status":"complete","review":"synthetic","abilities":[
        {"kind":"static","face":0_i64,"line":1_i64,"body":{"op":"keyword","name":"guard"}},
        {"kind":"static","face":1_i64,"line":1_i64,"body":{"op":"restrict","subjects":"self","action":"ignore_guard"}}
    ]});
    let bright = json!({"name":"bright","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"-","power":"4","hp":"6","traits":[],"text":null,"sections":[]});
    let mut dark = bright.clone();
    dark["name"] = json!("dark");
    let printed = json!({"number":"unit-dual","faces":[bright,dark]});
    Arc::new(
        Catalog::from_documents(
            &format!("{}\n{printed}", snapshot()),
            &registry(),
            &[("dual.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    )
}

#[test]
fn evolved_information_uses_the_linked_visible_face_after_load_play_and_observation() {
    for face in [0_i64, 1_i64] {
        for pre_evolved in [false, true] {
            let loaded = dual_evolution_catalog();
            let mut position = setup();
            position["players"]["P2"]["zones"]["field"][0]["state"] =
                json!({"acted":true,"keywords":["guard"]});
            let zone = if pre_evolved {
                "evolution"
            } else {
                "evolve_deck"
            };
            position["players"]["P1"]["zones"][zone] =
                json!([{"id":"e","card":"unit-dual","state":{"face":face}}]);
            if pre_evolved {
                position["players"]["P1"]["zones"]["field"][0]["state"] =
                    json!({"evolved":true,"evolved_with":"e"});
            }
            let mut engine = Game::new(
                Arc::clone(&loaded),
                &position,
                &Value::Null,
                &Value::Null,
                "dual",
            )
            .unwrap();
            if !pre_evolved {
                let choice = engine
                    .legal()
                    .unwrap()
                    .into_iter()
                    .find(|choice| {
                        choice["do"] == "evolve" && choice["face"].as_i64().unwrap_or(0) == face
                    })
                    .unwrap();
                assert_eq!(
                    engine.decide(&choice, "evolve").unwrap().outcome,
                    "resolved"
                );
            }
            let packet = engine.projection(View::P1).unwrap();
            assert_eq!(packet["objects"]["a"]["face"], face);
            assert_eq!(
                packet["objects"]["a"]["name"],
                if face == 0 { "bright" } else { "dark" }
            );
            assert_eq!(
                packet["objects"]["a"]["keywords"],
                if face == 0 {
                    json!(["守護"])
                } else {
                    json!([])
                }
            );
            let saved: Game =
                serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
            let sampled = Game::from_observation(loaded, &packet, "P1", "dual").unwrap();
            for mut copy in [engine, saved, sampled] {
                assert_eq!(
                    copy.projection(View::P1).unwrap()["objects"]["a"]["name"],
                    packet["objects"]["a"]["name"]
                );
                let result = copy
                    .decide(
                        &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
                        "attack",
                    )
                    .unwrap();
                assert_eq!(
                    result.outcome,
                    if face == 0 {
                        "cannot-attack"
                    } else {
                        "resolved"
                    }
                );
            }
        }
    }
}

#[test]
fn ignoring_guard_still_checks_posture_target_legality_and_ability_loss() {
    let mut position = setup();
    position["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"evolved":true,"evolved_with":"e"});
    position["players"]["P1"]["zones"]["evolution"] =
        json!([{"id":"e","card":"unit-dual","state":{"face":1_i64}}]);
    position["players"]["P2"]["zones"]["field"] = json!([
        {"id":"guard","card":"unit-follower","state":{"acted":true,"keywords":["guard"]}},
        {"id":"standing","card":"unit-follower"},
        {"id":"resting","card":"unit-follower","state":{"acted":true}}
    ]);
    for (target, allowed) in [
        ("P2.leader", true),
        ("guard", true),
        ("resting", true),
        ("standing", false),
        ("P1.leader", false),
    ] {
        let mut engine = Game::new(
            dual_evolution_catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "guard",
        )
        .unwrap();
        let result = engine
            .decide(
                &json!({"do":"attack","attacker":"a","target":target}),
                "attack",
            )
            .unwrap();
        assert_eq!(
            result.outcome,
            if allowed { "resolved" } else { "cannot-attack" }
        );
    }
    position["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(true);
    let mut acted = Game::new(
        dual_evolution_catalog(),
        &position,
        &Value::Null,
        &Value::Null,
        "acted",
    )
    .unwrap();
    assert_eq!(
        acted
            .decide(
                &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
                "attack"
            )
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    position["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    position["players"]["P1"]["zones"]["field"][0]["state"]["silenced"] = json!(true);
    let mut silenced = Game::new(
        dual_evolution_catalog(),
        &position,
        &Value::Null,
        &Value::Null,
        "silenced",
    )
    .unwrap();
    assert_eq!(
        silenced
            .decide(
                &json!({"do":"attack","attacker":"a","target":"P2.leader"}),
                "attack"
            )
            .unwrap()
            .outcome,
        "cannot-attack"
    );
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "Synthetic EX observers distinguish individual entry events from trigger limits."
)]
fn ex_observer_catalog(body: &Value, limited: bool) -> Arc<Catalog> {
    let mut doc = document(body);
    doc["cards"]["unit-spell"]["abilities"][0]["targets"] = json!([]);
    let mut trigger = json!({"kind":"trigger","line":1_i64,"event":"ex_enter","subject":{"side":"self","zone":"ex"},"body":{"op":"modify","subjects":"self.leader","hp":1_i64}});
    if limited {
        trigger["limit"] = json!(1_i64);
        trigger["limit_at"] = json!("trigger");
    }
    doc["cards"]["unit-follower"]["abilities"] = json!([trigger]);
    doc["cards"]["unit-token"] = json!({"status":"complete","review":"synthetic","abilities":[]});
    let token = json!({"number":"unit-token","faces":[{"name":"entry-token","card_class":"ニュートラル","card_type":"フォロワー・トークン","cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]}]});
    Arc::new(
        Catalog::from_documents(
            &format!("{}\n{token}", snapshot()),
            &registry(),
            &[("ex.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    )
}

#[test]
fn simultaneous_ex_creation_produces_one_distinguishable_trigger_per_created_object() {
    let loaded = ex_observer_catalog(
        &json!({"op":"create","name":"entry-token","count":2_i64,"to":"ex"}),
        false,
    );
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &setup(),
        &Value::Null,
        &Value::Null,
        "ex-batch",
    )
    .unwrap();
    let result = engine
        .decide(&json!({"do":"play","card":"s"}), "create")
        .unwrap();
    let pending = result
        .events
        .iter()
        .filter(|event| event["kind"] == "待機")
        .collect::<Vec<_>>();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0]["event"]["entered_ex"], "new-1");
    assert_eq!(pending[1]["event"]["entered_ex"], "new-2");
    assert_eq!(pending[0]["group"], pending[1]["group"]);
    assert_eq!(
        engine.query(View::P1, "P1.leader.life").unwrap(),
        Some(json!(20_i64))
    );
    let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let sampled =
        Game::from_observation(loaded, &engine.projection(View::P1).unwrap(), "P1", "ex").unwrap();
    for mut resumed in [saved, sampled] {
        for object in ["new-2", "new-1"] {
            resumed.decide(&json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1_i64},"event":{"entered_ex":object}}}),"trigger").unwrap();
        }
        assert_eq!(
            resumed.query(View::P1, "P1.leader.life").unwrap(),
            Some(json!(22_i64))
        );
        assert_eq!(
            resumed.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(20_i64))
        );
    }
}

#[test]
fn sequential_ex_entries_obey_trigger_limits_and_wait_for_the_surrounding_effect() {
    let move_one =
        json!({"op":"move","subjects":{"side":"self","zone":"deck","top":1_i64},"to":"ex"});
    let loaded = ex_observer_catalog(
        &json!({"op":"seq","steps":[move_one,{"op":"optional","then":{"op":"seq","steps":[]}},move_one]}),
        true,
    );
    let mut position = setup();
    position["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"first","card":"unit-follower"},{"id":"second","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &position,
        &Value::Null,
        &Value::Null,
        "ex-sequence",
    )
    .unwrap();
    assert_eq!(
        engine
            .decide(&json!({"do":"play","card":"s"}), "move")
            .unwrap()
            .outcome,
        "paused"
    );
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(packet["awaiting"]["at"], "resolve");
    assert_eq!(packet["P1"]["leader"]["life"], 20_i64);
    assert_eq!(
        packet["semantic_state"]["pending_triggers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        packet["semantic_state"]["used_this_turn"][0]["count"],
        1_i64
    );
    let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let sampled = Game::from_observation(loaded, &packet, "P1", "ex").unwrap();
    for mut resumed in [saved, sampled] {
        let result = resumed
            .decide(
                &json!({"do":"resolve-choice","choice":"decline"}),
                "continue",
            )
            .unwrap();
        assert_eq!(result.outcome, "resolved");
        assert!(result.events.iter().all(|event| event["kind"] != "待機"));
        assert_eq!(
            resumed.query(View::P1, "P1.ex_count").unwrap(),
            Some(json!(2_i64))
        );
        assert_eq!(resumed.legal().unwrap().len(), 1);
        resumed
            .decide(
                &json!({"do":"choose-pending","pending":{"ability":{"source":"a","line":1_i64}}}),
                "trigger",
            )
            .unwrap();
        assert_eq!(
            resumed.query(View::P1, "P1.leader.life").unwrap(),
            Some(json!(21_i64))
        );
    }
}

#[test]
fn limited_damage_prevention_preserves_charges_for_zero_and_roundtrips_after_consumption() {
    let pause = json!({"op":"optional","then":{"op":"seq","steps":[]}});
    let loaded = Arc::new(catalog(&json!({"op":"seq","steps":[
        {"op":"replace_damage","subjects":"self.leader","prevent":true,"uses":2_i64,"until":"end-of-turn"},
        {"op":"damage","subjects":"self.leader","amount":0_i64},
        pause,
        {"op":"damage","subjects":"self.leader","amount":3_i64},
        pause,
        {"op":"damage","subjects":"self.leader","amount":4_i64},
        {"op":"damage","subjects":"self.leader","amount":2_i64}
    ]})));
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &setup(),
        &Value::Null,
        &Value::Null,
        "charges",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(
        engine.projection(View::P1).unwrap()["semantic_state"]["continuous_effects"][0]["effect"]["uses"],
        2_i64
    );
    engine
        .decide(
            &json!({"do":"resolve-choice","choice":"decline"}),
            "first-hit",
        )
        .unwrap();
    let packet = engine.projection(View::P1).unwrap();
    assert_eq!(
        packet["semantic_state"]["continuous_effects"][0]["effect"]["uses"],
        1_i64
    );
    assert_eq!(packet["P1"]["leader"]["life"], 20_i64);
    let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let sampled = Game::from_observation(loaded, &packet, "P1", "charges").unwrap();
    for mut resumed in [saved, sampled] {
        let result = resumed
            .decide(
                &json!({"do":"resolve-choice","choice":"decline"}),
                "last-hits",
            )
            .unwrap();
        assert_eq!(
            resumed.query(View::P1, "P1.leader.life").unwrap(),
            Some(json!(18_i64))
        );
        let hits = result
            .events
            .iter()
            .filter(|event| event["kind"] == "ダメージ")
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["amount"], 2_i64);
        assert_eq!(
            resumed.projection(View::P1).unwrap()["semantic_state"]["continuous_effects"][0]["effect"]
                ["uses"],
            0_i64
        );
    }
}

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Synthetic replacement abilities provide distinct references for ordering."
)]
fn limited_replacement_catalog(prevent: bool) -> Arc<Catalog> {
    let mut doc = document(&json!({"op":"seq","steps":[]}));
    let mut abilities = Vec::new();
    for line in [1_i64, 2_i64] {
        abilities.push(json!({"kind":"activated","line":line,"body":{"op":"replace_damage","subjects":"both.leaders","uses":1_i64,"prevent":prevent,"amount":-1_i64,"until":"end-of-turn"}}));
    }
    abilities.push(json!({"kind":"activated","line":3_i64,"body":{"op":"damage","subjects":"both.leaders","amount":6_i64}}));
    doc["cards"]["unit-follower"]["abilities"] = json!(abilities);
    Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry(),
            &[("limited.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    )
}

#[test]
fn simultaneous_damage_waits_for_every_order_before_charging_and_applying() {
    let loaded = limited_replacement_catalog(false);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &setup(),
        &Value::Null,
        &Value::Null,
        "batch",
    )
    .unwrap();
    for line in [1_i64, 2_i64] {
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":line}}),
                "shield",
            )
            .unwrap();
    }
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":3_i64}}),
            "hits",
        )
        .unwrap();
    let order = json!({"do":"order-replacements","order":[{"source":"a","line":1_i64},{"source":"a","line":2_i64}]});
    assert_eq!(
        engine.decide(&order, "first-order").unwrap().outcome,
        "paused"
    );
    let packet = engine.projection(View::P2).unwrap();
    assert_eq!(packet["awaiting"]["by"], "P2");
    assert_eq!(packet["P1"]["leader"]["life"], 20_i64);
    assert!(
        packet["semantic_state"]["continuous_effects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["effect"]["uses"] == 1_i64)
    );
    let before = engine.digest().unwrap();
    assert_eq!(
        engine
            .decide(&json!({"do":"order-replacements","order":[]}), "invalid")
            .unwrap()
            .outcome,
        "cannot-play"
    );
    assert_eq!(engine.digest().unwrap(), before);
    let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    assert!(matches!(
        Game::from_observation(Arc::clone(&loaded), &packet, "P2", "batch").unwrap_err(),
        EngineFailure::Unsupported(_)
    ));
    let sampled =
        Game::from_observation(loaded, &engine.projection(View::P1).unwrap(), "P1", "batch")
            .unwrap();
    for mut resumed in [saved, sampled] {
        let result = resumed.decide(&order, "second-order").unwrap();
        assert_eq!(result.outcome, "resolved");
        let hits = result
            .events
            .iter()
            .filter(|event| event["kind"] == "ダメージ")
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0]["group"], hits[1]["group"]);
        assert!(hits.iter().all(|hit| hit["amount"] == 4_i64));
        resumed
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":3_i64}}),
                "unshielded",
            )
            .unwrap();
        for seat in ["P1", "P2"] {
            assert_eq!(
                resumed
                    .query(View::P1, &format!("{seat}.leader.life"))
                    .unwrap(),
                Some(json!(10_i64))
            );
        }
    }
}

#[test]
fn a_prevented_hit_does_not_consume_the_other_limited_replacement() {
    let mut engine = Game::new(
        limited_replacement_catalog(true),
        &setup(),
        &Value::Null,
        &Value::Null,
        "stack",
    )
    .unwrap();
    for line in [1_i64, 2_i64] {
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":line}}),
                "shield",
            )
            .unwrap();
    }
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":3_i64}}),
            "first",
        )
        .unwrap();
    let order = json!({"do":"order-replacements","order":[{"source":"a","line":2_i64},{"source":"a","line":1_i64}]});
    for _ in 0_u8..2 {
        engine.decide(&order, "order").unwrap();
    }
    let packet = engine.projection(View::P1).unwrap();
    for entry in packet["semantic_state"]["continuous_effects"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            entry["effect"]["uses"],
            i64::from(entry["context"]["reference"]["line"] == 1_i64)
        );
    }
    let second = engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":3_i64}}),
            "second",
        )
        .unwrap();
    assert_eq!(second.outcome, "resolved");
    assert!(
        second
            .events
            .iter()
            .all(|event| event["kind"] != "ダメージ")
    );
    engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":3_i64}}),
            "third",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P1, "P1.leader.life").unwrap(),
        Some(json!(14_i64))
    );
}

#[test]
fn indistinguishable_limited_replacement_instances_fail_closed_without_spending() {
    let mut engine = Game::new(
        limited_replacement_catalog(true),
        &setup(),
        &Value::Null,
        &Value::Null,
        "ambiguous",
    )
    .unwrap();
    for _ in 0_u8..2 {
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                "shield",
            )
            .unwrap();
    }
    let before = engine.digest().unwrap();
    let error = engine
        .decide(
            &json!({"do":"activate","ability":{"source":"a","line":3_i64}}),
            "damage",
        )
        .unwrap_err();
    assert!(matches!(error, EngineFailure::Unsupported(_)));
    assert_eq!(engine.digest().unwrap(), before);
}

#[test]
fn optional_selection_is_one_input_and_decline_clears_the_binding() {
    let body = json!({"op":"seq","steps":[
        {"op":"select","select":"target.1","min":1_i64,"max":1_i64,"bind":"chosen"},
        {"op":"optional","selection":{"select":{"side":"opponent","zone":"field"},"min":1_i64,"max":1_i64,"bind":"chosen"},"then":{"op":"modify","subjects":"chosen","power":3_i64}},
        {"op":"damage","subjects":"opponent.leader","amount":{"count":"chosen"}}
    ]});
    let loaded = Arc::new(catalog(&body));
    let mut position = setup();
    position["players"]["P2"]["zones"]["field"] =
        json!([{"id":"b","card":"unit-follower"},{"id":"c","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &position,
        &Value::Null,
        &Value::Null,
        "optional",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    engine
        .decide(
            &json!({"do":"resolve-choice","select":["b"]}),
            "previous-binding",
        )
        .unwrap();
    assert_eq!(
        engine.legal().unwrap(),
        vec![
            json!({"do":"resolve-choice","choice":"execute","select":["b"]}),
            json!({"do":"resolve-choice","choice":"execute","select":["c"]}),
            json!({"do":"resolve-choice","choice":"decline"})
        ]
    );
    for decision in [
        json!({"do":"resolve-choice","select":[]}),
        json!({"do":"resolve-choice","choice":"execute"}),
        json!({"do":"resolve-choice","choice":"decline","select":["b"]}),
    ] {
        let before = engine.digest().unwrap();
        assert_eq!(
            engine.decide(&decision, "invalid").unwrap().outcome,
            "cannot-play"
        );
        assert_eq!(engine.digest().unwrap(), before);
    }
    let packet = engine.projection(View::P1).unwrap();
    let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let sampled = Game::from_observation(loaded, &packet, "P1", "optional").unwrap();
    for resumed in [saved, sampled] {
        for execute in [true, false] {
            let mut branch = resumed.clone();
            let choice = if execute {
                json!({"do":"resolve-choice","choice":"execute","select":["c"]})
            } else {
                json!({"do":"resolve-choice","choice":"decline"})
            };
            assert_eq!(
                branch.decide(&choice, "choose").unwrap().outcome,
                "resolved"
            );
            assert_eq!(
                branch.query(View::P1, "P2.leader.life").unwrap(),
                Some(json!(if execute { 19_i64 } else { 20_i64 }))
            );
            assert_eq!(
                branch.query(View::P1, "P2.field.c.power").unwrap(),
                Some(json!(if execute { 5_i64 } else { 2_i64 }))
            );
            assert_eq!(
                branch.query(View::P1, "P2.field.b.power").unwrap(),
                Some(json!(2_i64))
            );
        }
    }
}

#[test]
fn optional_selection_without_a_complete_candidate_distinguishes_decline_from_zero() {
    for minimum in [0_i64, 1_i64] {
        let mut engine = game(
            &json!({"op":"optional","selection":{"select":{"side":"self","zone":"hand"},"min":minimum,"max":1_i64,"bind":"chosen"},"then":{"op":"damage","subjects":"opponent.leader","amount":5_i64}}),
        );
        engine
            .decide(
                &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
                "cast",
            )
            .unwrap();
        let decline = json!({"do":"resolve-choice","choice":"decline"});
        let execute = json!({"do":"resolve-choice","choice":"execute","select":[]});
        let mut choices = Vec::new();
        if minimum == 0 {
            choices.push(execute.clone());
            let mut branch = engine.clone();
            branch.decide(&execute, "execute-zero").unwrap();
            assert_eq!(
                branch.query(View::P1, "P2.leader.life").unwrap(),
                Some(json!(15_i64))
            );
        }
        choices.push(decline.clone());
        assert_eq!(engine.legal().unwrap(), choices);
        engine.decide(&decline, "skip").unwrap();
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(20_i64))
        );
    }
}

#[test]
fn compound_selection_and_position_use_the_post_draw_state_in_one_input() {
    let choice = json!({"op":"choice","timing":"resolve","min":1_i64,"max":1_i64,
        "selection":{"select":{"side":"self","zone":"hand"},"min":1_i64,"max":1_i64,"bind":"chosen"},
        "labels":[{"position":"top"},{"position":"bottom"}],
        "modes":[{"op":"move","subjects":"chosen","to":"deck","position":"top"},{"op":"move","subjects":"chosen","to":"deck","position":"bottom"}]
    });
    let loaded = Arc::new(catalog(
        &json!({"op":"seq","steps":[{"op":"draw","count":1_i64},choice]}),
    ));
    let mut position = setup();
    position["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"s","card":"unit-spell"},{"id":"h","card":"unit-follower"}]);
    position["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"d","card":"unit-follower"},{"id":"e","card":"unit-follower"}]);
    let mut engine = Game::new(
        Arc::clone(&loaded),
        &position,
        &Value::Null,
        &Value::Null,
        "compound",
    )
    .unwrap();
    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    let choices = engine.legal().unwrap();
    assert_eq!(choices.len(), 4);
    for card in ["d", "h"] {
        for destination in ["top", "bottom"] {
            assert!(
                choices.contains(
                    &json!({"do":"resolve-choice","select":[card],"position":destination})
                )
            );
        }
    }
    assert_eq!(
        engine
            .decide(&json!({"do":"resolve-choice","select":["h"]}), "incomplete")
            .unwrap()
            .outcome,
        "cannot-play"
    );
    let saved: Game = serde_json::from_str(&serde_json::to_string(&engine).unwrap()).unwrap();
    let sampled = Game::from_observation(
        loaded,
        &engine.projection(View::P1).unwrap(),
        "P1",
        "compound",
    )
    .unwrap();
    for resumed in [saved, sampled] {
        for destination in ["top", "bottom"] {
            let mut branch = resumed.clone();
            assert_eq!(
                branch
                    .decide(
                        &json!({"do":"resolve-choice","select":["d"],"position":destination}),
                        "choose"
                    )
                    .unwrap()
                    .outcome,
                "resolved"
            );
            let omniscient = branch.projection(View::Referee).unwrap();
            let deck = omniscient["P1"]["deck"].as_array().unwrap();
            assert_eq!(
                if destination == "top" {
                    deck.first()
                } else {
                    deck.last()
                },
                Some(&json!("d"))
            );
            assert_eq!(
                branch.query(View::P1, "P1.hand").unwrap(),
                Some(json!(["h"]))
            );
        }
    }
}

#[test]
fn compound_order_preserves_the_choosing_players_private_deck_order() {
    let loaded = Arc::new(catalog(
        &json!({"op":"choice","timing":"resolve","by":"opponent","min":1_i64,"max":1_i64,
            "selection":{"select":{"side":"opponent","zone":"field"},"min":2_i64,"max":2_i64,"order":true,"bind":"chosen"},
            "labels":[{"position":"top"},{"position":"bottom"}],
            "modes":[{"op":"move","subjects":"chosen","to":"deck","position":"top"},{"op":"move","subjects":"chosen","to":"deck","position":"bottom"}]
        }),
    ));
    let mut position = setup();
    position["players"]["P2"]["zones"]["field"] =
        json!([{"id":"b","card":"unit-follower"},{"id":"c","card":"unit-follower"}]);
    let mut engine = Game::new(loaded, &position, &Value::Null, &Value::Null, "order").unwrap();

    engine
        .decide(
            &json!({"do":"play","card":"s","targets":{"1":["b"]}}),
            "cast",
        )
        .unwrap();
    assert_eq!(engine.projection(View::P2).unwrap()["awaiting"]["by"], "P2");
    assert_eq!(engine.legal().unwrap().len(), 4);
    engine
        .decide(
            &json!({"do":"resolve-choice","order":["b","c"],"position":"bottom"}),
            "order",
        )
        .unwrap();
    assert_eq!(
        engine.query(View::P2, "P2.deck").unwrap(),
        Some(json!(["b", "c"]))
    );
    let hidden = engine.projection(View::P1).unwrap();
    assert!(
        !hidden["knowledge"]["located"]
            .as_array()
            .unwrap()
            .contains(&json!("b"))
    );
    assert!(hidden["objects"].get("b").is_none());
}

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Synthetic counters isolate aggregate payment from printed-card identities."
)]
fn counter_payment_catalog(costs: &Value, body: &Value) -> Arc<Catalog> {
    let mut doc = document(&json!({"op":"seq","steps":[]}));
    doc["cards"]["unit-follower"]["abilities"] =
        json!([{"kind":"activated","line":1_i64,"costs":costs,"body":body}]);
    let mut registry_doc: Value = serde_json::from_str(&registry()).unwrap();
    registry_doc["keywords"]["fusion_counter"] = json!({"ja":"融合カウンター","role":"counter"});
    Arc::new(
        Catalog::from_documents(
            &snapshot(),
            &registry_doc.to_string(),
            &[("counters.yaml".into(), doc.to_string())],
        )
        .unwrap(),
    )
}

#[test]
fn counter_costs_reserve_the_aggregate_and_reject_partial_payment_without_any_events() {
    let loaded = counter_payment_catalog(
        &json!([
            {"op":"pp","amount":1_i64},
            {"op":"counter","subjects":"self","name":"fusion_counter","amount":-2_i64},
            {"op":"counter","subjects":"self","name":"fusion_counter","amount":-2_i64}
        ]),
        &json!({"op":"damage","subjects":"opponent.leader","amount":4_i64}),
    );
    for counters in [3_i64, 4_i64] {
        let mut position = setup();
        position["players"]["P1"]["zones"]["field"][0]["state"]["counters"] =
            json!({"融合カウンター":counters});
        let mut engine = Game::new(
            Arc::clone(&loaded),
            &position,
            &Value::Null,
            &Value::Null,
            "payment",
        )
        .unwrap();
        let action = json!({"do":"activate","ability":{"source":"a","line":1_i64}});
        assert_eq!(engine.legal().unwrap().contains(&action), counters == 4);
        let before = engine.digest().unwrap();
        let result = engine.decide(&action, "activate").unwrap();
        if counters == 3 {
            assert_eq!(result.outcome, "cannot-activate");
            assert!(result.events.is_empty());
            assert_eq!(engine.digest().unwrap(), before);
        } else {
            assert_eq!(result.outcome, "resolved");
            assert_eq!(
                engine.query(View::P1, "P1.pp.current").unwrap(),
                Some(json!(1_i64))
            );
            assert_eq!(
                engine
                    .query(View::P1, "P1.field.a.counters.融合カウンター")
                    .unwrap(),
                Some(json!(0_i64))
            );
            assert_eq!(
                engine.query(View::P1, "P2.leader.life").unwrap(),
                Some(json!(16_i64))
            );
            let payments = result
                .events
                .iter()
                .filter(|event| event["kind"] == "カウンター")
                .collect::<Vec<_>>();
            assert_eq!(payments.len(), 2);
            assert!(payments.iter().all(|event| event["delta"] == -2_i64));
            assert_eq!(payments[0]["group"], payments[1]["group"]);
            assert_eq!(
                engine.decide(&action, "retry").unwrap().outcome,
                "cannot-activate"
            );
        }
    }
}

#[test]
fn zero_counter_cost_is_paid_but_partial_counter_effects_report_only_the_actual_change() {
    for (counters, amount) in [(0_i64, 0_i64), (2, -5), (0, -5)] {
        let loaded = counter_payment_catalog(
            &json!([{ "op":"counter","subjects":"self","name":"fusion_counter","amount":0_i64}]),
            &json!({"op":"if_done","attempt":{"op":"counter","subjects":"self","name":"fusion_counter","amount":amount},"then":{"op":"damage","subjects":"opponent.leader","amount":1_i64}}),
        );
        let mut position = setup();
        position["players"]["P1"]["zones"]["field"][0]["state"]["counters"] =
            json!({"融合カウンター":counters});
        let mut engine = Game::new(loaded, &position, &Value::Null, &Value::Null, "zero").unwrap();
        let result = engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1_i64}}),
                "activate",
            )
            .unwrap();
        assert_eq!(result.outcome, "resolved");
        assert!(
            result
                .events
                .iter()
                .any(|event| event["kind"] == "費用成立" && event["zero"] == true)
        );
        let changes = result
            .events
            .iter()
            .filter(|event| event["kind"] == "カウンター")
            .collect::<Vec<_>>();
        assert_eq!(changes.len(), usize::from(counters > 0));
        if let Some(event) = changes.first() {
            assert_eq!(event["delta"], -counters);
        }
        assert_eq!(
            engine.query(View::P1, "P2.leader.life").unwrap(),
            Some(json!(if counters > 0 { 19_i64 } else { 20_i64 }))
        );
    }
}
