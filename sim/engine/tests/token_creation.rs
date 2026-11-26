//! Small synthetic selections exercise capacity without rebuilding the repository.

#![allow(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Fixture assertions can panic and pattern checks only handle the expected variants."
)]
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

#[path = "support/rule_fixtures.rs"]
mod rule_fixtures;

#[path = "token_creation/banish_observed.rs"]
mod banish_observed;
#[path = "token_creation/paid_x.rs"]
mod paid_x;
#[path = "token_creation/per.rs"]
mod per;
#[path = "token_creation/strict.rs"]
mod strict;
#[path = "token_creation/victory.rs"]
mod victory;

#[path = "support/current_fixture.rs"]
mod current_fixture;
use current_fixture::author;

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
        json!({"format":1_u8,"kind":"keyword_registry","rule_names":{}}).to_string(),
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
        "format: 1\nkind: fixture_cards\ncards:\n  creator-alt:\n    status: complete\n"
    ]]);
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
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
    let keywords = json!({"format":1_u8,"kind":"keyword_registry","rule_names":{}}).to_string();
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

#[path = "token_creation/ward_bypass.rs"]
mod ward_bypass;

#[test]
fn selection_construction_names_replace_printed_collab_names() {
    let (snapshot, keywords, mut input) = selection(1);
    let mut collab: Value = serde_json::from_str(snapshot.lines().next().unwrap()).unwrap();
    collab["number"] = json!("creator-collab");
    collab["faces"][0]["name"] = json!("collab printed name");
    let snapshot = format!("{snapshot}{collab}\n");
    input["authors"][0]["printings"] = json!(["creator", "creator-collab"]);
    input["authors"][0]["rules_names"] = json!(["creator rules name"]);
    input["authors"][1]["rules_names"] = json!(["token rules name"]);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let mut state = initial(0);
    state["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"creator","card":"creator"},
        {"id":"collab","card":"creator-collab"}
    ]);
    let mut game =
        Game::from_selected_effects(selected, &state, &Value::Null, &Value::Null, "names").unwrap();
    assert!(matches!(
        game.play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Resolved(_)
    ));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["collab"]["name"], "creator rules name");
    let token = view["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(view["objects"][token]["name"], "token rules name");
    input["authors"][0]["unexpected"] = json!(true);
    Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
}
