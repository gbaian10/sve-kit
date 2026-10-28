//! Small synthetic selections exercise capacity without rebuilding the repository.

#![expect(
    clippy::unwrap_used,
    reason = "Synthetic fixture failures must stop the test."
)]

use serde_json::{Value, json};
use sve_engine::catalog::Catalog;
use sve_engine::game::internal::{CardPlay, InstructionOutcome};
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
    let input = json!({"status":"accepted","input":{
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
