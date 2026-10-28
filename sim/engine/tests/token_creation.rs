//! Small synthetic selections exercise capacity without rebuilding the repository.

#![expect(
    clippy::unwrap_used,
    reason = "Synthetic fixture failures must stop the test."
)]

extern crate alloc;

use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_engine::catalog::Catalog;
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
    for (occupied, expected) in [(0_usize, 2_usize), (4, 1), (5, 0)] {
        let (snapshot, keywords, mut input) = selection(2);
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

#[test]
fn only_successful_field_creations_emit_per_token_entry_triggers() {
    for (occupied, expected) in [(1_usize, 2_usize), (4, 1), (5, 0)] {
        let (mut snapshot, keywords, mut input) = selection(2);
        input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
            ["abilities"][0]["spell"]["do"][0]["create"]["to"] = json!("field");
        let (observer, source) = author(
            "observer",
            "33333333333333333333333333333333",
            "フォロワー",
            vec![
                json!({"auto":{"on":{"enter_field":{"who":"self","sel":{"kind":"follower"}}},"do":[{"draw":{"n":1_u8}}]}}),
            ],
        );
        snapshot.push_str(&observer.to_string());
        snapshot.push('\n');
        input["authors"].as_array_mut().unwrap().push(source);
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
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
            assert!(options.iter().all(|option| option.card == "observer"));
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
        assert_eq!(triggers, expected);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), expected);
        assert_eq!(
            view["P1"]["field"].as_array().unwrap().len(),
            occupied + expected
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
