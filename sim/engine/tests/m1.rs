//! M1: load-time rejection, the engine behaviours it made explicit, and the R1–R3
//! properties from the design cross-review (docs/m0/known-errors.md).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::default_numeric_fallback,
    clippy::too_many_lines,
    clippy::shadow_unrelated,
    clippy::format_push_string,
    reason = "Synthetic fixtures are literal JSON; a construction error or missing field must fail the test."
)]

extern crate alloc;

use alloc::sync::Arc;

use serde_json::{Value, json};
use sve_engine::EngineFailure;
use sve_engine::catalog::Catalog;
use sve_engine::game::{Game, View};

/// `(number, card type, cost, traits)`; every card is named after its number.
const CARDS: &[(&str, &str, &str, &[&str])] = &[
    ("f-a", "フォロワー", "1", &[]),
    ("f-b", "フォロワー", "3", &["財宝"]),
    ("f-c", "フォロワー", "1", &["兵士"]),
    ("amulet", "アミュレット", "1", &[]),
    ("hurt", "スペル", "1", &[]),
    ("heal", "スペル", "1", &[]),
    ("spell", "スペル", "1", &[]),
    ("fuser", "フォロワー", "2", &[]),
];

fn snapshot() -> String {
    CARDS
        .iter()
        .map(|(number, kind, cost, traits)| {
            json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,
                "cost":cost,"power":"2","hp":"3","traits":traits,"text":null,"sections":[]}]})
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn registry() -> String {
    json!({"version":"astra/1","keywords":{
        "guard":{"ja":"守護","rule":"12.8","expansion":{"op":"keyword","name":"guard"}},
        "fusion":{"ja":"融合","role":"ability-label"}
    }})
    .to_string()
}

/// Programs keyed by card number; cards not given get no abilities.
fn catalog(programs: &Value) -> Catalog {
    let mut cards = serde_json::Map::new();
    for (number, ..) in CARDS {
        let abilities = programs.get(*number).cloned().unwrap_or_else(|| json!([]));
        cards.insert(
            (*number).into(),
            json!({"status":"complete","review":"synthetic","abilities":abilities}),
        );
    }
    let document = json!({"version":"astra/1","cards":cards});
    let loaded = Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("unit.yaml".into(), document.to_string())],
    )
    .unwrap();
    assert!(loaded.rejections().is_empty(), "{:?}", loaded.rejections());
    loaded
}

fn zones(zones: &Value) -> Value {
    let mut all = json!({"deck":[{"id":format!("deck-{}", zones["tag"].as_str().unwrap_or("x")),"card":"f-a"}]});
    for (zone, objects) in zones.as_object().into_iter().flatten() {
        if zone != "tag" {
            all[zone] = objects.clone();
        }
    }
    all
}

fn setup(p1: &Value, p2: &Value) -> Value {
    let tagged = |zones: &Value, tag: &str| {
        let mut zones = zones.clone();
        if zones.get("tag").is_none() {
            zones["tag"] = json!(tag);
        }
        zones
    };
    let (p1, p2) = (&tagged(p1, "p1"), &tagged(p2, "p2"));
    json!({
        "turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":3,"P2":2},"phase":"main"},
        "history":"explicit",
        "players":{
            "P1":{"leader":{"class":"ニュートラル","life":15},"pp":{"current":10,"max":10},"ep":0,"sep":0,
                  "construction":"class","zones":zones(p1)},
            "P2":{"leader":{"class":"ニュートラル","life":15},"pp":{"current":10,"max":10},"ep":0,"sep":0,
                  "construction":"class","zones":zones(p2)}
        }
    })
}

fn start(catalog: Catalog, setup: &Value) -> Game {
    Game::new(Arc::new(catalog), setup, &Value::Null, &Value::Null, "m1").unwrap()
}

fn life(engine: &Game, seat: &str) -> Value {
    engine
        .query(View::Referee, &format!("{seat}.leader.life"))
        .unwrap()
        .unwrap()
}

/// Plays `card`, then resolves every trigger it formed; returns the 待機 count.
fn play_and_settle(engine: &mut Game, card: &str) -> usize {
    let step = engine
        .decide(&json!({"do":"play","card":card}), card)
        .unwrap();
    assert!(
        matches!(step.outcome.as_str(), "resolved" | "paused"),
        "{step:?}"
    );
    let mut formed = step
        .events
        .iter()
        .filter(|event| event["kind"] == "待機")
        .count();
    while let Some(choice) = engine
        .legal()
        .unwrap()
        .into_iter()
        .find(|choice| choice["do"] == "choose-pending")
    {
        let step = engine.decide(&choice, "pending").unwrap();
        formed = formed.saturating_add(
            step.events
                .iter()
                .filter(|event| event["kind"] == "待機")
                .count(),
        );
    }
    formed
}

// ---------------------------------------------------------------- load-time rejection

/// A YAML document where one card carries the construct under test at a known line.
fn yaml_document(body: &str) -> String {
    let mut text = String::from("version: astra/1\ncards:\n");
    for (number, ..) in CARDS {
        if *number == "spell" {
            text.push_str(&format!(
                "  spell:\n    status: complete\n    review: synthetic\n    abilities:\n{body}"
            ));
        } else {
            text.push_str(&format!(
                "  {number}:\n    status: complete\n    review: synthetic\n    abilities: []\n"
            ));
        }
    }
    text
}

fn rejections(body: &str) -> Vec<String> {
    let loaded = Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("unit.yaml".into(), yaml_document(body))],
    )
    .unwrap();
    loaded
        .rejections()
        .get("spell")
        .cloned()
        .unwrap_or_default()
}

#[test]
fn schema_valid_but_unknown_names_are_rejected_with_card_and_line() {
    let spell =
        |inner: &str| format!("      - line: 1\n        kind: spell\n        body:\n{inner}");
    let cases = [
        (
            "          op: damage\n          subjects: opponent.leader\n          amount:\n            read: self.turn.evolved\n",
            "turn counter `evolved`",
        ),
        (
            "          op: damage\n          subjects: opponent.leader\n          amount:\n            read: self.pp.maximum\n",
            "unknown reference `self.pp`",
        ),
        (
            "          op: damage\n          subjects:\n            side: opponent\n            zone: field\n            type: folower\n          amount: 1\n",
            "unknown card type `folower`",
        ),
        (
            "          op: damage\n          subjects: chosenn\n          amount: 1\n",
            "unknown reference `chosenn`",
        ),
        (
            "          op: modify\n          subjects: self.leader\n          hp: 1\n          until: next-turn\n",
            "has no expiry",
        ),
        (
            "          op: aura\n          subjects: self\n          power: 1\n",
            "not implemented in Exec position",
        ),
        (
            "          op: restrict\n          subjects: opponent.leader\n          action: target\n",
            "no rule procedure checks",
        ),
        (
            "          op: draw\n          count: 1\n          up_to: true\n",
            "`draw.up_to` is not implemented",
        ),
        (
            "          op: damage\n          subjects: opponent.leader\n          amount:\n            read: event.amount\n",
            "read outside a trigger",
        ),
    ];
    for (body, expected) in cases {
        let found = rejections(&spell(body));
        assert_eq!(found.len(), 1, "{body}: {found:?}");
        assert!(found[0].contains(expected), "{found:?}");
        // The line points into the offending card, not to 0.
        let line = found[0]
            .strip_prefix("unit.yaml:")
            .and_then(|rest| rest.split(':').next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap();
        assert!(line > 30, "{found:?}");
    }
    let trigger = "      - line: 1\n        kind: trigger\n        event: enterd\n        subject: self\n        body:\n          op: draw\n          count: 1\n";
    assert!(rejections(trigger)[0].contains("trigger event `enterd`"));
    let wrong_field = "      - line: 1\n        kind: trigger\n        event: enter\n        subject: self\n        body:\n          op: damage\n          subjects: opponent.leader\n          amount:\n            read: event.after_life\n";
    assert!(rejections(wrong_field)[0].contains("event `enter` has no field `after_life`"));
    assert!(
        rejections(&spell(
            "          op: damage\n          subjects: opponent.leader\n          amount: 1\n"
        ))
        .is_empty()
    );
}

#[test]
fn a_rejected_program_is_never_executed() {
    let text = "      - line: 1\n        kind: spell\n        body:\n          op: damage\n          subjects: opponent.leader\n          amount:\n            read: self.turn.evolved\n";
    let loaded = Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("unit.yaml".into(), yaml_document(text))],
    )
    .unwrap();
    let mut engine = start(
        loaded,
        &setup(&json!({"hand":[{"id":"s","card":"spell"}]}), &json!({})),
    );
    let before = engine.digest().unwrap();
    match engine.decide(&json!({"do":"play","card":"s"}), "play") {
        Err(EngineFailure::Unsupported(message)) => {
            assert!(message.contains("rejected at load"), "{message}");
        }
        other => panic!("expected a load rejection, got {other:?}"),
    }
    assert_eq!(engine.digest().unwrap(), before);
}

// ---------------------------------------------------------------- behaviours made explicit

#[test]
fn draw_restriction_blocks_effect_draws_outside_the_start_phase() {
    let loaded = catalog(&json!({
        "amulet":[{"line":1,"kind":"static","body":{"op":"restrict","subjects":"opponent.leader","action":"draw",
            "condition":{"fn":"ne","args":[{"read":"turn.phase"},"start"]}}}],
        "spell":[{"line":1,"kind":"spell","body":{"op":"draw","side":"both","count":1}}]
    }));
    let mut engine = start(
        loaded,
        &setup(
            &json!({"tag":"p1","field":[{"id":"w","card":"amulet"}],"hand":[{"id":"s","card":"spell"}]}),
            &json!({"tag":"p2"}),
        ),
    );
    engine
        .decide(&json!({"do":"play","card":"s"}), "draw")
        .unwrap();
    assert_eq!(
        engine.query(View::Referee, "P1.hand").unwrap(),
        Some(json!(["deck-p1"]))
    );
    assert_eq!(
        engine.query(View::Referee, "P2.hand").unwrap(),
        Some(json!([]))
    );
}

#[test]
fn banish_card_play_and_fusion_events_reach_their_triggers() {
    let hit = json!({"op":"damage","subjects":"opponent.leader","amount":1});
    let loaded = catalog(&json!({
        "f-a":[{"line":1,"kind":"trigger","event":"banish","subject":"self","active_zones":["hand"],"body":hit}],
        "amulet":[
            {"line":1,"kind":"trigger","event":"card_play","subject":{"side":"self","zone":"any","type":"spell"},"body":hit},
            {"line":2,"kind":"trigger","event":"fusion","subject":{"side":"self","zone":"any"},
             "trigger_if":{"read":"event.fused_trait.財宝"},"body":hit}
        ],
        "spell":[{"line":1,"kind":"spell","body":{"op":"banish","subjects":{"side":"self","zone":"hand","name":"f-a"}}}],
        "fuser":[{"line":1,"kind":"activated","keyword":"fusion","active_zones":["hand"],
            "cost_selections":[{"key":"1","select":{"side":"self","zone":"hand","other":true},"min":1,"max":1}],
            "costs":[{"op":"move","subjects":"cost.1","to":"cemetery"}],
            "body":{"op":"damage","subjects":"opponent.leader","amount":0}}]
    }));
    // Played spell (card_play) + banished-from-hand card (banish): 2 damage to P2.
    let mut engine = start(
        loaded.clone(),
        &setup(
            &json!({"field":[{"id":"w","card":"amulet"}],"hand":[{"id":"s","card":"spell"},{"id":"x","card":"f-a"}]}),
            &json!({}),
        ),
    );
    assert_eq!(play_and_settle(&mut engine, "s"), 2);
    assert_eq!(life(&engine, "P2"), json!(13));
    // Fusion with a 財宝 material triggers; with another material it does not.
    for (material, expected) in [("f-b", 14), ("f-c", 15)] {
        let mut engine = start(
            loaded.clone(),
            &setup(
                &json!({"field":[{"id":"w","card":"amulet"}],"hand":[{"id":"u","card":"fuser"},{"id":"m","card":material}]}),
                &json!({}),
            ),
        );
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"u","line":1},"costs":{"1":["m"]}}),
                "fuse",
            )
            .unwrap();
        while let Some(choice) = engine
            .legal()
            .unwrap()
            .into_iter()
            .find(|choice| choice["do"] == "choose-pending")
        {
            engine.decide(&choice, "pending").unwrap();
        }
        assert_eq!(life(&engine, "P2"), json!(expected), "{material}");
    }
}

#[test]
fn name_alias_counts_for_name_selectors() {
    let loaded = catalog(&json!({
        "f-a":[{"line":1,"kind":"static","body":{"op":"name_alias","subjects":"self","name":"ゴースト","while_zone":"any"}}],
        "spell":[{"line":1,"kind":"spell","body":{"op":"damage","subjects":"opponent.leader",
            "amount":{"count":{"side":"self","zone":"field","name":"ゴースト"}}}}]
    }));
    let mut engine = start(
        loaded,
        &setup(
            &json!({"field":[{"id":"g","card":"f-a"},{"id":"n","card":"f-c"}],"hand":[{"id":"s","card":"spell"}]}),
            &json!({}),
        ),
    );
    engine
        .decide(&json!({"do":"play","card":"s"}), "count")
        .unwrap();
    assert_eq!(life(&engine, "P2"), json!(14));
}

#[test]
fn search_groups_restrict_the_combinations() {
    let loaded = catalog(&json!({
        "spell":[{"line":1,"kind":"spell","body":{"op":"search",
            "select":{"side":"self","zone":"deck","type":"follower"},"min":0,"max":2,"to":"hand",
            "groups":[
                {"key":"1","select":{"side":"self","zone":"deck","type":"follower","where":{"fn":"le","args":[{"read":"item.cost"},3]}},"min":0,"max":1},
                {"key":"1","select":{"side":"self","zone":"deck","type":"follower","trait":"兵士"},"min":0,"max":1}
            ]}}]
    }));
    let mut engine = start(
        loaded,
        &setup(
            &json!({"hand":[{"id":"s","card":"spell"}],
                "deck":[{"id":"d1","card":"f-a"},{"id":"d2","card":"f-b"},{"id":"d3","card":"f-c"}]}),
            &json!({}),
        ),
    );
    engine
        .decide(&json!({"do":"play","card":"s"}), "search")
        .unwrap();
    let mut chosen = engine
        .legal()
        .unwrap()
        .into_iter()
        .map(|choice| choice["select"].clone())
        .collect::<Vec<_>>();
    chosen.sort_by_key(Value::to_string);
    // d1 and d2 (both only cost ≤ 3) cannot be taken together; d3 is the 兵士.
    assert!(!chosen.contains(&json!(["d1", "d2"])), "{chosen:?}");
    assert!(chosen.contains(&json!(["d1", "d3"])), "{chosen:?}");
    assert!(chosen.contains(&json!(["d2", "d3"])), "{chosen:?}");
}

#[test]
fn replace_choice_widens_the_choice_bounds_of_its_controller() {
    let hit = |n: i64| json!({"op":"damage","subjects":"opponent.leader","amount":n});
    let spell = json!([{"line":1,"kind":"spell","body":{"op":"choice","min":1,"max":1,"modes":[hit(1),hit(2),hit(4)]}}]);
    let widened = catalog(&json!({
        "amulet":[{"line":1,"kind":"static","body":{"op":"replace_choice","side":"self","min":0,
            "max":{"read":"choice.mode_count"},"condition":{"fn":"ge","args":[{"read":"choice.min"},1]}}}],
        "spell":spell
    }));
    let options = |loaded: Catalog| {
        let engine = start(
            loaded,
            &setup(
                &json!({"field":[{"id":"w","card":"amulet"}],"hand":[{"id":"s","card":"spell"}]}),
                &json!({}),
            ),
        );
        engine
            .legal()
            .unwrap()
            .into_iter()
            .filter(|choice| choice["card"] == "s")
            .count()
    };
    assert_eq!(options(catalog(&json!({"spell":spell}))), 3);
    // 0..=3 of three modes.
    assert_eq!(options(widened), 8);
}

#[test]
fn next_opponent_turn_end_expires_after_that_turn() {
    let loaded = catalog(&json!({
        "spell":[{"line":1,"kind":"spell","body":{"op":"replace_damage","subjects":"self.leader","prevent":true,
            "kind":"any","until":"next-opponent-turn-end"}}]
    }));
    let mut engine = start(
        loaded,
        &setup(
            &json!({"tag":"p1","hand":[{"id":"s","card":"spell"}]}),
            &json!({"tag":"p2"}),
        ),
    );
    engine
        .decide(&json!({"do":"play","card":"s"}), "shield")
        .unwrap();
    let active = |engine: &Game| {
        serde_json::to_value(engine).unwrap()["state"]["continuous"]
            .as_array()
            .map_or(0, Vec::len)
    };
    assert_eq!(active(&engine), 1);
    engine.decide(&json!({"do":"end-phase"}), "p1-end").unwrap();
    settle_turn(&mut engine);
    assert_eq!(active(&engine), 1, "still during the opponent's turn");
    engine.decide(&json!({"do":"end-phase"}), "p2-end").unwrap();
    settle_turn(&mut engine);
    assert_eq!(active(&engine), 0, "gone after the opponent's turn ended");
}

fn settle_turn(engine: &mut Game) {
    while let Some(choice) = engine
        .legal()
        .unwrap()
        .into_iter()
        .find(|choice| choice["do"] != "play" && choice["do"] != "end-phase")
    {
        if choice["do"] == "activate" || choice["do"] == "attack" || choice["do"] == "evolve" {
            break;
        }
        engine.decide(&choice, "settle").unwrap();
    }
}

// ---------------------------------------------------------------- R1–R3 properties

/// R1: a once-per-turn trigger is counted only when a pending ability actually forms.
/// An event that fails the trigger's own event condition (here: life went up, not
/// down) must not use the turn's only use, in any order of events.
#[test]
fn r1_failed_trigger_condition_never_consumes_the_turn_limit() {
    let loaded = catalog(&json!({
        "amulet":[{"line":1,"kind":"trigger","event":"leader_life_change","subject":{"side":"self","zone":"leader"},
            "limit":1,"limit_at":"trigger",
            "trigger_if":{"fn":"lt","args":[{"read":"event.after_life"},{"read":"event.before_life"}]},
            "body":{"op":"damage","subjects":"opponent.leader","amount":1}}],
        "hurt":[{"line":1,"kind":"spell","body":{"op":"damage","subjects":"self.leader","amount":1}}],
        "heal":[{"line":1,"kind":"spell","body":{"op":"modify","subjects":"self.leader","hp":1}}]
    }));
    for mask in 0_u32..16 {
        let order = (0..4)
            .map(|bit| {
                if mask & (1 << bit) == 0 {
                    "heal"
                } else {
                    "hurt"
                }
            })
            .collect::<Vec<_>>();
        let hand = order
            .iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("c{i}"),"card":card}))
            .collect::<Vec<_>>();
        let mut engine = start(
            loaded.clone(),
            &setup(
                &json!({"field":[{"id":"w","card":"amulet"}],"hand":hand}),
                &json!({}),
            ),
        );
        let mut formed = Vec::new();
        for i in 0..order.len() {
            formed.push(play_and_settle(&mut engine, &format!("c{i}")));
        }
        let hurts = i64::try_from(order.iter().filter(|card| **card == "hurt").count()).unwrap();
        let heals = 4_i64 - hurts;
        assert_eq!(
            life(&engine, "P1"),
            json!(15_i64 - hurts + heals),
            "{order:?}"
        );
        let first_hurt = order.iter().position(|card| *card == "hurt");
        let expected = (0..order.len())
            .map(|i| usize::from(Some(i) == first_hurt))
            .collect::<Vec<_>>();
        assert_eq!(formed, expected, "{order:?}");
    }
}

/// R2: every selector field the loader accepts changes the result when the only
/// candidate fails it; none is silently ignored.
#[test]
fn r2_every_accepted_selector_field_filters_or_is_rejected() {
    let base = json!({"side":"self","zone":"field"});
    let failing = [
        ("type", json!("amulet")),
        ("trait", json!("財宝")),
        ("name", json!("f-b")),
        ("name_contains", json!("zzz")),
        ("not_name", json!("f-a")),
        ("keyword", json!("guard")),
        ("other", json!(true)),
        ("top", json!(0)),
        ("where", json!({"fn":"gt","args":[{"read":"item.cost"},5]})),
        ("ability_event", json!("attack")),
    ];
    let count_with = |field: Option<(&str, &Value)>| {
        let mut selector = base.clone();
        if let Some((key, value)) = field {
            selector[key] = value.clone();
        }
        let loaded = catalog(&json!({
            "f-a":[{"line":1,"kind":"activated","body":{"op":"damage","subjects":"opponent.leader",
                "amount":{"count":selector}}}]
        }));
        assert!(loaded.rejections().is_empty(), "{:?}", loaded.rejections());
        let mut engine = start(
            loaded,
            &setup(&json!({"field":[{"id":"a","card":"f-a"}]}), &json!({})),
        );
        engine
            .decide(
                &json!({"do":"activate","ability":{"source":"a","line":1}}),
                "count",
            )
            .unwrap();
        15_i64 - life(&engine, "P2").as_i64().unwrap()
    };
    assert_eq!(count_with(None), 1);
    for (key, value) in &failing {
        assert_eq!(count_with(Some((key, value))), 0, "{key} was ignored");
    }
    // A field outside the schema never reaches the engine at all.
    let mut selector = base;
    selector["colour"] = json!("red");
    let document = json!({"version":"astra/1","cards":{"f-a":{"status":"complete","review":"x","abilities":[
        {"line":1,"kind":"spell","body":{"op":"damage","subjects":selector,"amount":1}}]}}});
    Catalog::from_documents(
        &snapshot(),
        &registry(),
        &[("unit.yaml".into(), document.to_string())],
    )
    .unwrap_err();
}

/// R3: the AI's sampled world depends only on what the seat observed: two games that
/// differ only in hidden cards give the same world for the same seed.
#[test]
fn r3_same_observation_and_seed_give_the_same_world() {
    let loaded = Arc::new(catalog(&json!({})));
    for (left, right) in [("f-a", "f-b"), ("spell", "amulet"), ("f-c", "heal")] {
        let world = |hidden: &str| {
            let game = Game::new(
                Arc::clone(&loaded),
                &setup(
                    &json!({"field":[{"id":"a","card":"f-a"}]}),
                    &json!({"hand":[{"id":"h","card":hidden}],"deck":[{"id":"d","card":hidden}]}),
                ),
                &Value::Null,
                &Value::Null,
                "table",
            )
            .unwrap();
            let observed = game.projection(View::P1).unwrap();
            let sampled =
                Game::from_observation(Arc::clone(&loaded), &observed, "P1", "same-seed").unwrap();
            (observed, sampled.digest().unwrap())
        };
        let (seen_left, world_left) = world(left);
        let (seen_right, world_right) = world(right);
        assert_eq!(
            seen_left, seen_right,
            "{left}/{right} must look the same to P1"
        );
        assert_eq!(
            world_left, world_right,
            "{left}/{right} leaked into the world"
        );
    }
}

/// R-0009: a long-form "〜とき、…なら" ability triggers on the event alone; the
/// condition is read when it resolves, and a false condition just does nothing.
#[test]
fn long_form_condition_triggers_and_is_checked_at_resolution() {
    let loaded = catalog(&json!({
        "amulet":[{"line":1,"kind":"trigger","event":"end","side":"self",
            "body":{"op":"if","condition":{"fn":"eq","args":[{"count":{"side":"self","zone":"hand"}},0]},
                "then":{"op":"damage","subjects":"opponent.leader","amount":3}}}]
    }));
    for (hand, expected) in [(json!([]), 12), (json!([{"id":"h","card":"f-a"}]), 15)] {
        let mut engine = start(
            loaded.clone(),
            &setup(
                &json!({"field":[{"id":"w","card":"amulet"}],"hand":hand}),
                &json!({}),
            ),
        );
        let step = engine.decide(&json!({"do":"end-phase"}), "end").unwrap();
        assert!(
            step.events.iter().any(|event| event["kind"] == "待機"),
            "the ability forms even when the condition is false: {step:?}"
        );
        settle_turn(&mut engine);
        assert_eq!(life(&engine, "P2"), json!(expected));
    }
}
