//! Small synthetic selections exercise capacity without rebuilding the repository.

#![expect(
    clippy::unwrap_used,
    reason = "Synthetic fixture failures must stop the test."
)]

extern crate alloc;

use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_engine::catalog::{Catalog, SelectedEffects};
use sve_engine::game::internal::{Answer, CardPlay, InputDomain, InstructionOutcome};
use sve_engine::game::{Game, View};
use sve_engine::ir::PlayerId;

fn author(number: &str, id: &str, kind: &str, mut abilities: Vec<Value>) -> (Value, Value) {
    let card = format!("c:{id}");
    let face = format!("f:{id}");
    let revision = format!("revision-{number}");
    let lines = (1..=abilities.len()).collect::<Vec<_>>();
    for (index, ability) in abilities.iter_mut().enumerate() {
        let line = index.checked_add(1).unwrap();
        let fields = ability.as_object_mut().unwrap();
        fields.insert("line".into(), json!(line));
        fields.insert("ports".into(), json!({"imports":{},"exports":{}}));
        fields.insert(
            "sources".into(),
            json!([{
                "admission":"light","owner":{"kind":"face_revision","revision_id":revision},
                "face_id":face,"field":"effect","ordinal":null,"line":line,
                "scope":"ability_0","covers":[line]
            }]),
        );
    }
    let face_fact = json!({"name":number,"card_class":"ニュートラル","card_type":kind,
        "cost":"1","power":"1","hp":"2","traits":[],"text":"","sections":[]});
    let input = json!({"status":"accepted","printings":[number],"input":{
        "admission":"direct","card_id":card,"card_no":number,
        "source_payload":{"card_id":card,"faces":[{
            "face_id":face,"revision_id":revision,"ordinal":0_u8,"side":"front","cost":1_u8,
            "fields":[{"field":"effect","ordinal":null,"lines":lines}]
        }]},
        "tree":{"format":1_u8,"kind":"author_effect_set","scope":"shared","cards":{number:{
            "meta":{"dsl":"1.0","source":"direct","admission":"light",
                "written_by":[{"actor":{"who":"test","model":null},"date":"2026-10-28"}],
                "reviewed_by":[],"status":"draft","verified_by_exam":[],"rulings":[]},
            "faces":{face:{"abilities":abilities}}
        }}}
    }});
    (json!({"number":number,"faces":[face_fact]}), input)
}

fn selection(count: u64) -> (String, String, Value) {
    let (creator, source) = author(
        "creator",
        "11111111111111111111111111111111",
        "スペル",
        vec![
            json!({"spell":{"do":[{"create":{"token":"token","to":"ex","n":count,"result":"made"}}]}}),
        ],
    );
    let (token, template) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        vec![],
    );
    (
        format!("{creator}\n{token}\n"),
        json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string(),
        json!({"documents":[],"authors":[source,template]}),
    )
}

fn initial(occupied: usize) -> Value {
    let existing = (0..occupied)
        .map(|index| json!({"id":format!("old-{index}"),"card":"token"}))
        .collect::<Vec<_>>();
    json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main",
        "elapsed_turns":{"P1":3_u8,"P2":3_u8}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,
            "zones":{"hand":[{"id":"creator","card":"creator"}],"ex":existing}},
        "P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
    }})
}

fn token_kind(snapshot: &str, kind: &str) -> String {
    let mut result = snapshot
        .lines()
        .map(|line| {
            let mut card: Value = serde_json::from_str(line).unwrap();
            if card.get("number").and_then(Value::as_str) == Some("token") {
                let face = card
                    .get_mut("faces")
                    .and_then(Value::as_array_mut)
                    .and_then(|faces| faces.first_mut())
                    .and_then(Value::as_object_mut)
                    .unwrap();
                face.insert("card_type".into(), json!(kind));
                if kind != "フォロワー・トークン" {
                    face.insert("power".into(), json!("-"));
                    face.insert("hp".into(), json!("-"));
                }
            }
            card.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    result.push('\n');
    result
}

#[test]
fn field_creation_keeps_spell_and_crest_tokens_closed() {
    for kind in ["スペル・トークン", "クレスト・トークン"] {
        let (snapshot, keywords, mut input) = selection(1);
        let snapshot = token_kind(&snapshot, kind);
        input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
            ["abilities"][0]["spell"]["do"][0]["create"]["to"] = json!("field");
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        let failure = Game::from_selected_effects(
            selected,
            &initial(0),
            &Value::Null,
            &Value::Null,
            "field-token-kind",
        )
        .unwrap_err();
        assert!(
            failure
                .to_string()
                .contains("field creation requires a follower or amulet token"),
            "the new Amulet path must not grant field creation to other special cards"
        );
    }
}

#[test]
fn token_creation_composes_with_private_discard_bindings() {
    let (snapshot, keywords, mut input) = selection(2);
    let steps = &mut input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["spell"]["do"];
    steps.as_array_mut().unwrap().extend([
        json!({"draw":{"n":1_u8}}),
        json!({"discard":{"who":"self","n":1_u8}}),
    ]);
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
}

#[test]
fn keyword_before_token_creation_and_discard_keeps_bindings_unique() {
    let (_, _, mut input) = selection(2);
    let (creator, source) = author(
        "creator",
        "11111111111111111111111111111111",
        "フォロワー",
        vec![
            json!({"keywords":["ward"]}),
            json!({"fanfare":{"do":[
                {"create":{"token":"token","to":"ex","n":2_u8}},
                {"draw":{"n":1_u8}},
                {"discard":{"who":"self","n":1_u8}}
            ]}}),
        ],
    );
    let (token, _) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        vec![],
    );
    input["authors"][0] = source;
    Catalog::from_current_selection(
        &format!("{creator}\n{token}\n"),
        include_str!("../../../authored/rules/keywords.yaml"),
        &input,
    )
    .unwrap();
}

#[test]
fn token_creation_rejects_ambiguous_and_reused_result_bindings() {
    for second in [
        json!({"create":{"token":"token","to":"ex","result":"made"}}),
        json!({"create":{"token":"token","to":"ex","bind":"made"}}),
        json!({"create":{"token":"token","to":"ex","result":"other","bind":"also"}}),
    ] {
        let (snapshot, keywords, mut input) = selection(2);
        input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]
            ["f:11111111111111111111111111111111"]["abilities"][0]["spell"]["do"]
            .as_array_mut()
            .unwrap()
            .push(second);
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    }
}

#[test]
fn fixed_token_creation_reports_only_tokens_that_fit_in_own_ex() {
    for (kind, occupied, expected) in [
        ("フォロワー・トークン", 0_usize, 2_usize),
        ("フォロワー・トークン", 4, 1),
        ("フォロワー・トークン", 5, 0),
        ("クレスト・トークン", 0, 1),
        ("クレスト・トークン", 1, 0),
    ] {
        let (snapshot, keywords, input) = selection(2);
        let snapshot = snapshot.replace("フォロワー・トークン", kind);
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        let mut game = Game::from_selected_effects(
            selected,
            &initial(occupied),
            &Value::Null,
            &Value::Null,
            "token-capacity",
        )
        .unwrap();
        let CardPlay::Resolved(result) = game
            .play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("single-name creation requires no capacity choice");
        };
        let created = result
            .progress
            .receipts
            .values()
            .filter_map(|outcome| {
                if let InstructionOutcome::Token(outcome) = outcome {
                    Some(outcome.created.len())
                } else {
                    None
                }
            })
            .sum::<usize>();
        assert_eq!(created, expected);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["ex"].as_array().unwrap().len(),
            occupied + expected
        );
        assert_eq!(view["P2"]["ex"], json!([]));
    }
}

#[test]
fn token_definition_links_require_a_complete_token_and_cannot_execute_as_mother_text() {
    let (snapshot, keywords, mut input) = selection(1);
    let author = &mut input["authors"][0]["input"];
    author["source_payload"]["faces"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "field":"section","ordinal":0_u8,"lines":[1_u8],"section_kind":"token_definition"
        }));
    let source = json!({"admission":"light","owner":{"kind":"face_revision","revision_id":"revision-creator"},
        "face_id":"f:11111111111111111111111111111111","field":"section","ordinal":0_u8,
        "line":1_u8,"scope":"ability_0","covers":[1_u8]});
    author["tree"]["cards"]["creator"]["token_sources"] =
        json!([{"source":source,"card_no":"token"}]);
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["token_sources"][0]["card_no"] =
        json!("creator");
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    let changed_author = &mut input["authors"][0]["input"];
    changed_author["tree"]["cards"]["creator"]["token_sources"][0]["card_no"] = json!("token");
    let ability = &mut changed_author["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0];
    ability["section"] = json!(0_u8);
    ability["sources"][0] = source;
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
}

#[test]
fn field_creation_caps_before_allocating_and_result_can_change_the_arrivals() {
    for (kind, occupied, expected) in [
        ("フォロワー・トークン", 0_usize, 2_usize),
        ("フォロワー・トークン", 4, 1),
        ("フォロワー・トークン", 5, 0),
        ("アミュレット・トークン", 0, 2),
        ("アミュレット・トークン", 4, 1),
        ("アミュレット・トークン", 5, 0),
    ] {
        let (snapshot, keywords, mut input) = selection(2);
        let snapshot = token_kind(&snapshot, kind);
        let steps = &mut input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
            ["abilities"][0]["spell"]["do"];
        steps[0]["create"]["to"] = json!("field");
        steps
            .as_array_mut()
            .unwrap()
            .push(json!({"act":{"ref":"made"}}));
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        let mut initial = initial(occupied);
        initial["players"]["P1"]["zones"]["field"] = initial["players"]["P1"]["zones"]["ex"].take();
        initial["players"]["P1"]["zones"]["ex"] = json!([]);
        let mut game = Game::from_selected_effects(
            selected,
            &initial,
            &Value::Null,
            &Value::Null,
            "field-capacity",
        )
        .unwrap();
        let CardPlay::Resolved(result) = game
            .play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("blank token has no choice")
        };
        let created = result
            .progress
            .receipts
            .values()
            .find_map(|outcome| {
                if let InstructionOutcome::Token(value) = outcome {
                    Some(value.created.len())
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(created, expected);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["field"].as_array().unwrap().len(),
            occupied + expected
        );
        assert_eq!(view["P1"]["ex"], json!([]));
        assert_eq!(view["P2"]["field"], json!([]));
        let arrivals = view["objects"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(id, _)| id.starts_with("new-"))
            .collect::<Vec<_>>();
        assert_eq!(arrivals.len(), expected);
        for (_, arrival) in arrivals {
            assert_eq!(arrival["acted"], true);
            assert_eq!(arrival["entered_this_turn"], true);
        }
    }
}

#[test]
fn field_creation_ward_choices_resume_the_same_objects_once() {
    let (snapshot, _, mut input) = selection(2);
    let (_, ward) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        vec![json!({"keywords":["ward"]})],
    );
    input["authors"][1] = ward;
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["spell"]["do"][0]["create"]["to"] = json!("field");
    let keywords = include_str!("../../../authored/rules/keywords.yaml");
    let selected = Catalog::from_current_selection(&snapshot, keywords, &input).unwrap();
    let mut game = Game::from_selected_effects(
        selected,
        &initial(0),
        &Value::Null,
        &Value::Null,
        "created-ward",
    )
    .unwrap();
    let mut play = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap();
    for acted in [true, false] {
        let CardPlay::Awaiting(question) = play else {
            panic!("Ward entry choice")
        };
        assert!(matches!(question.domain, InputDomain::WardEntry { .. }));
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(acted))
            .unwrap();
    }
    let CardPlay::Resolved(result) = play else {
        panic!("two choices complete the creation")
    };
    assert_eq!(
        result.progress.receipts.values().find_map(|outcome| {
            if let InstructionOutcome::Token(value) = outcome {
                Some(value.created.len())
            } else {
                None
            }
        }),
        Some(2)
    );
    let view = game.projection(View::Referee).unwrap();
    let ids = view["P1"]["field"].as_array().unwrap();
    assert_eq!(ids.len(), 2);
    let acted = ids
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["acted"] == true)
        .count();
    assert_eq!(acted, 1);
}

fn entry_selection(kind: &str, self_entry: bool) -> SelectedEffects {
    let (snapshot, keywords, mut input) = selection(2);
    let mut snapshot = token_kind(&snapshot, kind);
    *input.pointer_mut("/authors/0/input/tree/cards/creator/faces/f:11111111111111111111111111111111/abilities/0/spell/do/0/create/to").unwrap() = json!("field");
    let (observer, observer_source) = author(
        "observer",
        "33333333333333333333333333333333",
        "フォロワー",
        vec![
            json!({"auto":{"on":{"enter_field":{"who":"self","sel":{"kind":"follower"}}},"do":[{"draw":{"n":1_u8}}]}}),
        ],
    );
    snapshot.push_str(&observer.to_string());
    snapshot.push('\n');
    let authors = input
        .get_mut("authors")
        .and_then(Value::as_array_mut)
        .unwrap();
    authors.push(observer_source);
    if self_entry {
        let (_, token_source) = author(
            "token",
            "22222222222222222222222222222222",
            kind,
            vec![json!({"auto":{"on":{"enter_field":{"ref":"this"}},"do":[{"draw":{"n":1_u8}}]}})],
        );
        let token_author = authors
            .iter_mut()
            .find(|candidate| {
                candidate.pointer("/input/card_no").and_then(Value::as_str) == Some("token")
            })
            .unwrap();
        *token_author = token_source;
    }
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
}

#[test]
fn only_successful_field_creations_emit_per_token_entry_triggers() {
    for (kind, self_entry, occupied, created, expected) in [
        ("フォロワー・トークン", false, 1_usize, 2_usize, 2_usize),
        ("フォロワー・トークン", false, 4, 1, 1),
        ("フォロワー・トークン", false, 5, 0, 0),
        ("アミュレット・トークン", true, 1, 2, 2),
        ("アミュレット・トークン", true, 4, 1, 1),
        ("アミュレット・トークン", true, 5, 0, 0),
        ("アミュレット・トークン", false, 1, 2, 0),
    ] {
        let selected = entry_selection(kind, self_entry);
        let mut initial = initial(occupied - 1);
        let mut field = initial["players"]["P1"]["zones"]["ex"].take();
        field
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"observer","card":"observer"}));
        initial["players"]["P1"]["zones"]["field"] = field;
        initial["players"]["P1"]["zones"]["ex"] = json!([]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"draw-1","card":"creator"},{"id":"draw-2","card":"creator"},{"id":"draw-3","card":"creator"}
        ]);
        let mut game = Game::from_selected_effects(
            selected,
            &initial,
            &Value::Null,
            &Value::Null,
            "token-entry",
        )
        .unwrap();
        assert!(matches!(
            game.play_internal_card("creator", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Resolved(_)
        ));
        game.resume_internal_card().unwrap();
        let mut triggers = 0;
        while let Some(question) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("entry pending choice")
            };
            assert!(
                options
                    .iter()
                    .all(|option| option.card == if self_entry { "token" } else { "observer" })
            );
            let play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            assert!(matches!(play, CardPlay::Resolved(_)));
            triggers += 1;
            game.resume_internal_card().unwrap();
        }
        assert_eq!(
            triggers, expected,
            "successful Amulet entries trigger themselves, but never follower-only observers"
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), expected);
        assert_eq!(
            view["P1"]["field"].as_array().unwrap().len(),
            occupied + created
        );
    }
}

#[test]
fn created_result_does_not_mutate_a_token_after_it_changes_generation() {
    let (snapshot, keywords, mut input) = selection(1);
    let steps = &mut input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["spell"]["do"];
    steps[0]["create"]["to"] = json!("field");
    steps.as_array_mut().unwrap().extend([
        json!({"banish":{"ref":"made"}}),
        json!({"act":{"ref":"made"}}),
    ]);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let mut game = Game::from_selected_effects(
        selected,
        &initial(0),
        &Value::Null,
        &Value::Null,
        "stale-created",
    )
    .unwrap();
    let CardPlay::Resolved(result) = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("no choice")
    };
    assert!(result.progress.receipts.values().any(
        |outcome| matches!(outcome, InstructionOutcome::Posture(changes) if changes.is_empty())
    ));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!([]));
}

#[test]
fn printing_aliases_share_a_candidate_without_selecting_unrelated_numbers() {
    let (snapshot, keywords, mut input) = selection(1);
    let mut alias: Value = serde_json::from_str(snapshot.lines().next().unwrap()).unwrap();
    alias["number"] = json!("creator-alt");
    alias["faces"][0]["name"] = json!("alternate printed name");
    let mut unrelated = alias.clone();
    unrelated["number"] = json!("creator-lookalike");
    let mut token_alias: Value = serde_json::from_str(snapshot.lines().nth(1).unwrap()).unwrap();
    token_alias["number"] = json!("token-alt");
    let snapshot = format!("{snapshot}{alias}\n{unrelated}\n{token_alias}\n");
    input["authors"][1]["printings"] = json!(["token", "token-alt"]);
    input["authors"][0]["printings"] = json!(["creator", "creator-alt"]);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    assert!(Arc::ptr_eq(
        &selected.current["creator"],
        &selected.current["creator-alt"]
    ));
    assert!(!selected.current.contains_key("creator-lookalike"));
    let mut game =
        Game::from_selected_effects(selected, &initial(0), &Value::Null, &Value::Null, "aliases")
            .unwrap();
    assert!(matches!(
        game.play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Resolved(_)
    ));
    let state = game.projection(View::Referee).unwrap();
    let token = state["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(state["objects"][token]["card"], "token");
    for printings in [
        json!(["creator-alt"]),
        json!(["creator", "creator"]),
        json!(["creator", "missing"]),
    ] {
        input["authors"][0]["printings"] = printings;
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    }
    input["authors"][0]["printings"] = json!(["creator", "creator-alt"]);
    input["documents"] = json!([[
        "prototype.yaml",
        "format: 1\nkind: effect_set\ncards:\n  creator-alt:\n    status: complete\n    abilities: []\n"
    ]]);
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
}

#[test]
fn amulet_token_movement_keeps_other_special_card_kinds_closed() {
    for kind in ["アミュレット・トークン", "クレスト・トークン"] {
        let (snapshot, keywords, mut input) = selection(1);
        let snapshot = snapshot
            .lines()
            .map(|line| {
                let mut card: Value = serde_json::from_str(line).unwrap();
                if card["number"] == "token" {
                    card["faces"][0]["card_type"] = json!(kind);
                    card["faces"][0]["power"] = json!("-");
                    card["faces"][0]["hp"] = json!("-");
                }
                card.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]
            ["f:11111111111111111111111111111111"]["abilities"][0]["spell"]["do"]
            .as_array_mut()
            .unwrap()
            .push(json!({"move":{"ref":"made","to":"cemetery"}}));
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        let mut game = Game::from_selected_effects(
            selected,
            &initial(0),
            &Value::Null,
            &Value::Null,
            "amulet-token-movement",
        )
        .unwrap();
        let result = game.play_internal_card("creator", PlayerId { seat: 1 });
        if kind == "クレスト・トークン" {
            assert!(
                result.is_err(),
                "other special-card movement remains closed"
            );
        } else {
            assert!(matches!(result.unwrap(), CardPlay::Resolved(_)));
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["ex"], json!([]));
            assert_eq!(view["P1"]["cemetery"], json!(["creator"]));
            assert!(
                view["objects"]
                    .as_object()
                    .unwrap()
                    .values()
                    .all(|v| v["card"] != "token")
            );
        }
    }
}

#[test]
fn amulet_token_play_and_activation_use_ex_and_reject_a_hand_origin() {
    let (mut token, input) = author(
        "token",
        "22222222222222222222222222222222",
        "アミュレット・トークン",
        vec![
            json!({"activate":{"cost":{"pp":1_u8},"do":[{"move":{"ref":"this","to":"cemetery"}}]}}),
        ],
    );
    token["faces"][0]["power"] = json!("-");
    token["faces"][0]["hp"] = json!("-");
    let snapshot = token.to_string();
    let keywords = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let input = json!({"documents":[],"authors":[input]});
    for zone in ["ex", "hand"] {
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        let ability = selected.current["token"].program.card().abilities[0]
            .node
            .id;
        let mut position = initial(0);
        position["players"]["P1"]["zones"]["hand"] = json!([]);
        position["players"]["P1"]["zones"][zone] = json!([{"id":"token","card":"token"}]);
        let mut game = Game::from_selected_effects(
            selected,
            &position,
            &Value::Null,
            &Value::Null,
            "amulet-token-play",
        )
        .unwrap();
        let before = format!("{game:?}");
        let play = game.play_internal_card("token", PlayerId { seat: 1 });
        if zone == "hand" {
            assert!(play.is_err());
            assert!(
                format!("{game:?}") == before,
                "invalid token play must not pay or move"
            );
        } else {
            assert!(matches!(play.unwrap(), CardPlay::Entered(_)));
            game.resume_internal_card().unwrap();
            assert!(matches!(
                game.activate_internal_card("token", ability, PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Resolved(_)
            ));
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 8_u8);
            assert_eq!(view["P1"]["field"], json!([]));
            assert_eq!(view["P1"]["cemetery"], json!([]));
        }
    }
}

#[path = "token_creation/dynamic.rs"]
mod dynamic;

fn mixed_selection(steps: Value, ward: bool) -> SelectedEffects {
    let (mut snapshot, _, mut input) = selection(1);
    *input.pointer_mut("/authors/0/input/tree/cards/creator/faces/f:11111111111111111111111111111111/abilities/0/spell/do").unwrap() = steps;
    let (second, template) = author(
        "second",
        "44444444444444444444444444444444",
        "フォロワー・トークン",
        if ward {
            vec![json!({"keywords":["ward"]})]
        } else {
            vec![]
        },
    );
    snapshot.push_str(&second.to_string());
    snapshot.push('\n');
    input
        .get_mut("authors")
        .and_then(Value::as_array_mut)
        .unwrap()
        .push(template);
    Catalog::from_current_selection(
        &snapshot,
        include_str!("../../../authored/rules/keywords.yaml"),
        &input,
    )
    .unwrap()
}

#[expect(
    clippy::panic,
    reason = "Only a completed synthetic creation has receipts."
)]
fn token_receipts(play: &CardPlay) -> usize {
    let CardPlay::Resolved(result) = play else {
        panic!("creation must resolve");
    };
    result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Token(value) = outcome {
                Some(value.created.len())
            } else {
                None
            }
        })
        .sum()
}

#[expect(
    clippy::panic,
    reason = "The synthetic capacity case requires its exact question."
)]
fn choose_second_at_capacity(game: &mut Game, play: CardPlay) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("mixed capacity must ask which name fits");
    };
    let InputDomain::TokenCapacity { choices } = &question.domain else {
        panic!("capacity uses names, not objects");
    };
    assert_eq!(
        choices.len(),
        2,
        "identical copies must not duplicate name combinations"
    );
    let handle = u64::try_from(
        choices
            .iter()
            .position(|names| names == &["second"])
            .unwrap(),
    )
    .unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before
            .get("objects")
            .and_then(Value::as_object)
            .unwrap()
            .len(),
        5,
        "no token exists before the capacity answer"
    );
    assert!(
        game.answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::TokenCapacity(handle)
        )
        .is_err(),
        "another player cannot answer"
    );
    assert!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::TokenCapacity(u64::MAX)
        )
        .is_err(),
        "out-of-domain handles fail atomically"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "invalid answers preserve the exact question and state"
    );
    let resumed = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::TokenCapacity(handle),
        )
        .unwrap();
    assert!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::TokenCapacity(handle)
        )
        .is_err(),
        "the consumed question cannot create again"
    );
    resumed
}

#[test]
fn mixed_capacity_chooses_names_before_allocating_and_binds_only_arrivals() {
    for zone in ["field", "ex"] {
        for occupied in [0_usize, 4, 5] {
            let mut steps = vec![
                json!({"create":{"token":["token","second"],"n":2_u8,"to":zone,"result":"made"}}),
            ];
            if zone == "field" {
                steps.push(json!({"act":{"ref":"made"}}));
            }
            let selected = mixed_selection(json!(steps), false);
            let mut initial = initial(occupied);
            if zone == "field" {
                initial["players"]["P1"]["zones"]["field"] =
                    initial["players"]["P1"]["zones"]["ex"].take();
                initial["players"]["P1"]["zones"]["ex"] = json!([]);
            }
            let mut game = Game::from_selected_effects(
                selected,
                &initial,
                &Value::Null,
                &Value::Null,
                "mixed-capacity",
            )
            .unwrap();
            let mut play = game
                .play_internal_card("creator", PlayerId { seat: 1 })
                .unwrap();
            if occupied == 4 {
                play = choose_second_at_capacity(&mut game, play);
            }
            let expected = 4.min(5 - occupied);
            assert_eq!(
                token_receipts(&play),
                expected,
                "receipts count actual arrivals"
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"][zone].as_array().unwrap().len(),
                occupied + expected,
                "destination capacity is respected"
            );
            let arrivals = view["objects"]
                .as_object()
                .unwrap()
                .iter()
                .filter(|(id, _)| id.starts_with("new-"))
                .collect::<Vec<_>>();
            assert_eq!(
                arrivals.len(),
                expected,
                "discarded alternatives never allocate objects"
            );
            for (_, arrival) in arrivals {
                assert_eq!(
                    arrival["acted"],
                    zone == "field",
                    "the field result binds only successfully created members"
                );
                if occupied == 4 {
                    assert_eq!(
                        arrival["card"], "second",
                        "capacity may choose the later printed name"
                    );
                }
            }
        }
    }
}

#[test]
fn mixed_capacity_then_ward_resumes_the_same_batch_once() {
    let selected = mixed_selection(
        json!([
            {"create":{"token":["token","second"],"n":2_u8,"to":"field"}}
        ]),
        true,
    );
    let mut state = initial(4);
    state["players"]["P1"]["zones"]["field"] = state["players"]["P1"]["zones"]["ex"].take();
    state["players"]["P1"]["zones"]["ex"] = json!([]);
    let mut game =
        Game::from_selected_effects(selected, &state, &Value::Null, &Value::Null, "mixed-ward")
            .unwrap();
    let CardPlay::Awaiting(capacity) = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("capacity precedes Ward");
    };
    let InputDomain::TokenCapacity { choices } = capacity.domain else {
        panic!("name combinations");
    };
    let handle = u64::try_from(
        choices
            .iter()
            .position(|names| names == &["second"])
            .unwrap(),
    )
    .unwrap();
    let CardPlay::Awaiting(ward) = game
        .answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::TokenCapacity(handle),
        )
        .unwrap()
    else {
        panic!("chosen Ward token asks its entry posture");
    };
    assert!(
        matches!(ward.domain, InputDomain::WardEntry { .. }),
        "Ward is a separate rule procedure"
    );
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::TokenCapacity(handle)
        )
        .is_err(),
        "old capacity coordinate cannot answer Ward"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "stale answers do not allocate a second token"
    );
    let play = game
        .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(true))
        .unwrap();
    assert_eq!(
        token_receipts(&play),
        1,
        "Ward resumes the original one-member batch"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["field"].as_array().unwrap().len(),
        5,
        "the chosen token moves exactly once"
    );
}

#[test]
fn either_selects_at_resolution_after_payment_and_sequential_choices_have_new_epochs() {
    let branch = json!({"either":{"options":[
        [{"create":{"token":"token","to":"ex"}}],
        [{"create":{"token":"second","n":2_u8,"to":"ex"}}]
    ]}});
    let selected = mixed_selection(json!([branch, branch]), false);
    let mut game = Game::from_selected_effects(
        selected,
        &initial(0),
        &Value::Null,
        &Value::Null,
        "resolution-alternatives",
    )
    .unwrap();
    let mut play = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap();
    let mut previous = None;
    for pick in [1_usize, 0] {
        let CardPlay::Awaiting(question) = play else {
            panic!("each either resolves separately");
        };
        let InputDomain::Modes { modes, count, .. } = &question.domain else {
            panic!("resolution alternatives use the existing mode domain");
        };
        assert_eq!(count.0, 1, "exactly one printed alternative");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], 9_i32,
            "spell payment occurs before a resolution choice"
        );
        assert!(
            view["P1"]["hand"].as_array().unwrap().is_empty(),
            "the spell has already been played"
        );
        if let Some(token) = previous {
            assert_ne!(
                question.token, token,
                "sequential decisions have distinct coordinates"
            );
            assert!(
                game.answer_internal_card(question.actor, token, Answer::Modes(vec![modes[pick]]))
                    .is_err(),
                "an earlier alternative cannot answer the next"
            );
        }
        assert!(
            game.answer_internal_card(question.actor, question.token, Answer::Modes(vec![]))
                .is_err(),
            "required alternative cannot be declined"
        );
        assert!(
            game.answer_internal_card(question.actor, question.token, Answer::Modes(modes.clone()))
                .is_err(),
            "multiple alternatives cannot be combined"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            view,
            "invalid alternatives leave the state intact"
        );
        previous = Some(question.token);
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![modes[pick]]),
            )
            .unwrap();
    }
    assert_eq!(
        token_receipts(&play),
        3,
        "only the selected two-plus-one tokens are created"
    );
    let view = game.projection(View::Referee).unwrap();
    let cards = view["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            view["objects"][id.as_str().unwrap()]["card"]
                .as_str()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cards,
        ["second", "second", "token"],
        "resolution order follows each selected branch"
    );
}

#[test]
fn mixed_creation_checks_every_token_definition_and_rejects_empty_lists() {
    for names in [json!([]), json!(["token", "missing"])] {
        let (snapshot, keywords, mut input) = selection(1);
        *input.pointer_mut("/authors/0/input/tree/cards/creator/faces/f:11111111111111111111111111111111/abilities/0/spell/do/0/create/token").unwrap() = names;
        let loaded =
            Catalog::from_current_selection(&snapshot, &keywords, &input).and_then(|selected| {
                Game::from_selected_effects(
                    selected,
                    &initial(0),
                    &Value::Null,
                    &Value::Null,
                    "invalid-mixed",
                )
            });
        assert!(
            loaded.is_err(),
            "empty or incomplete mixed templates remain closed"
        );
    }
}

#[test]
fn resolution_alternative_can_use_an_earlier_created_set_without_leaking_branch_locals() {
    let selected = mixed_selection(
        json!([
            {"create":{"token":"token","to":"field","result":"earlier"}},
            {"either":{"options":[[{"act":{"ref":"earlier"}}],[{"create":{"token":"second","to":"ex","result":"branch_only"}}]]}},
            {"stand":{"ref":"earlier"}}
        ]),
        false,
    );
    let mut game = Game::from_selected_effects(
        selected,
        &initial(0),
        &Value::Null,
        &Value::Null,
        "resolution-local",
    )
    .unwrap();
    let CardPlay::Awaiting(question) = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("earlier creation precedes the resolution alternative");
    };
    let InputDomain::Modes { modes, .. } = &question.domain else {
        panic!("resolution modes");
    };
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "the outer created set already exists"
    );
    assert!(
        matches!(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![modes[0]])
            )
            .unwrap(),
            CardPlay::Resolved(_)
        ),
        "the whole spell resolves after the selected branch"
    );
    let view = game.projection(View::Referee).unwrap();
    let id = view["P1"]["field"][0].as_str().unwrap();
    assert_eq!(
        view["objects"][id]["acted"], false,
        "the original binding survives the branch and is stood afterwards"
    );
    assert_eq!(
        view["P1"]["ex"],
        json!([]),
        "the unselected branch creates nothing"
    );
}
