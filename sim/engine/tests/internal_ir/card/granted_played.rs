#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the granted observer scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use std::sync::OnceLock;
use sve_engine::game::internal::InputDomain;

fn installation() -> Value {
    // This models BP17-042's SuperEvolve clause, not its separate Look closure.
    json!({"grant":{"on":"this","until":"turn","abilities":[{
        "auto":{"on":{"played":{"who":"self","sel":{"trait":"機械"}}},"do":[
            {"damage":{"to":{"leaders":"opponents"},"n":1}},
            {"damage":{"to":{"who":"opp","zone":"field","kind":"follower"},"n":1}}
        ]}
    }]}})
}

fn programs() -> &'static BTreeMap<String, WellFormedCardProgram> {
    static PROGRAMS: OnceLock<BTreeMap<String, WellFormedCardProgram>> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        let mut input = equipment_played::tree(
            &json!({"line":1,"evolve":{"cost":{"pp":1}}}),
            "unit-follower",
        );
        let extra = equipment_played::tree(
            &json!({"line":2,"activate":{"cost":{"pp":0},"do":[installation()]}}),
            "unit-follower",
        );
        input["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(extra["cards"]["unit-follower"]["abilities"][0].clone());
        let form = equipment_played::tree(
            &json!({"super_evolve_time":{"do":[installation()]}}),
            "unit-form",
        );
        input["cards"]["unit-form"] = form["cards"]["unit-form"].clone();
        lowerer().lower(&input).unwrap()
    })
}

fn board() -> Game {
    board_with_spell(&card(vec![play::spell(vec![]), quick::permit()]))
}

fn board_with_spell(spell: &WellFormedCardProgram) -> Game {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-spell" {
                row["faces"][0]["traits"] = json!(["機械"]);
            }
            row
        })
        .collect::<Vec<_>>();
    let mut form = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    form["number"] = json!("unit-form");
    form["faces"][0]["card_type"] = json!("フォロワー・エボルヴ");
    rows.push(form);
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-form"] = json!({"status":"complete","abilities":[]});
    let catalog = Arc::new(
        Catalog::from_documents(
            &rows
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            &registry,
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P1");
    initial["turn"]["elapsed_turns"]["P1"] = json!(7);
    for side in ["P1", "P2"] {
        initial["players"][side]["pp"] = json!({"current":10,"max":10});
        initial["players"][side]["zones"]["hand"] = json!([]);
        initial["players"][side]["zones"]["deck"] = json!([
            {"id":format!("{side}-draw"),"card":"unit-amulet"}
        ]);
    }
    initial["players"]["P1"]["sep"] = json!(1);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"unit-form"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"spell0","card":"unit-spell"},{"id":"spell1","card":"unit-spell"},
        {"id":"mismatch","card":"unit-amulet"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "granted-played",
    )
    .unwrap();
    for (number, program) in programs() {
        game.load_internal_card(number, program).unwrap();
    }
    game.load_internal_card("unit-spell", spell).unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game
}

fn resolve_pending(game: &mut Game, count: usize) -> Vec<Activation> {
    let mut results = vec![];
    for remaining in (1..=count).rev() {
        event_ability::finish(game);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("granted pending")
        };
        assert_eq!(
            options.len(),
            remaining,
            "each installation retains one independent occurrence"
        );
        results.push(resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        ));
    }
    event_ability::finish(game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "all expected grant occurrences have resolved"
    );
    results
}

fn play_card(game: &mut Game, id: &str) {
    assert!(
        matches!(
            game.play_internal_card(id, PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_) | CardPlay::Resolved(_)
        ),
        "the fixture commits a proper card Play"
    );
    event_ability::finish(game);
}

#[test]
fn bp17_042_clause_grants_only_after_super_evolution_and_keeps_its_provider() {
    for super_evolution in [false, true] {
        let mut game = board();
        let mut pay = json!({"pp":1});
        if super_evolution {
            pay["sep"] = json!(1);
        }
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay}),
        )
        .unwrap();
        if super_evolution {
            resolve_pending(&mut game, 1);
        } else {
            event_ability::finish(&mut game);
        }
        play_card(&mut game, "mismatch");
        assert!(game.internal_pending_choice().unwrap().is_none());
        play_card(&mut game, "spell0");
        assert_eq!(
            game.internal_pending_choice().unwrap().is_some(),
            super_evolution
        );
        if super_evolution {
            let q = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &q.domain else {
                panic!("granted source")
            };
            assert_eq!(options[0].card, "unit-form");
            let result = resolve_pending(&mut game, 1).remove(0);
            assert_eq!(result.source.controller, PlayerId { seat: 1 });
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            20 - u8::from(super_evolution)
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            u8::from(super_evolution)
        );
    }
}

#[test]
fn separate_granted_played_installations_each_enqueue_and_expire_at_turn_end() {
    let mut game = board();
    let definition = programs()["unit-follower"].card().abilities[1].node.id;
    for _ in 0..2 {
        resolved(
            game.activate_internal_card("source", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        event_ability::finish(&mut game);
    }
    play_card(&mut game, "spell0");
    let results = resolve_pending(&mut game, 2);
    assert_ne!(results[0].source.instance, results[1].source.instance);
    assert_eq!(results[0].source.holder, results[1].source.holder);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("opponent-turn Quick")
    };
    assert_eq!(q.actor, PlayerId { seat: 1 });
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(options[0].handle)))
            .unwrap(),
    );
    event_ability::finish(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "expired grants cannot observe a later proper Play"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18
    );
}

#[test]
fn committed_granted_played_keeps_its_source_after_the_spell_moves_the_holder() {
    let returning = movement::put(1, zone(ZoneKind::Field, 50), OwnerZone::Hand);
    let mut game = board_with_spell(&card(vec![play::spell(vec![returning])]));
    let definition = programs()["unit-follower"].card().abilities[1].node.id;
    resolved(
        game.activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    event_ability::finish(&mut game);
    play_card(&mut game, "spell0");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!([])
    );
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("retained grant")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "unit-follower");
    let source = resolve_pending(&mut game, 1).remove(0).source;
    assert_eq!(source.controller, PlayerId { seat: 1 });
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        19
    );
    play_card(&mut game, "spell1");
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "a departed holder does not observe a later Play"
    );
}
