//! Python's direct writer reaches current selection and the ordinary card runtime.

#![expect(
    clippy::unwrap_used,
    reason = "Synthetic fixtures fail immediately on malformed test construction."
)]

use serde_json::{Value, json};
use std::{path::PathBuf, process::Command, sync::OnceLock};
use sve_engine::{
    catalog::Catalog,
    game::{Game, View, internal::CardPlay},
    ir::{PlayerId, lowering::ExpandedLowerer},
};

const FACE: &str = "f:22222222222222222222222222222222";

fn selection() -> &'static Value {
    static INPUT: OnceLock<Value> = OnceLock::new();
    INPUT.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../carddb");
        let output = Command::new("uv").args(["run", "--directory"]).arg(root).args([
            "--isolated", "--locked", "python", "-c",
            "from pathlib import Path\nfrom tempfile import TemporaryDirectory\nimport json\nfrom tests.support.direct_author_fixture import direct_author_input\nwith TemporaryDirectory() as root:\n print(json.dumps(direct_author_input(Path(root))))",
        ]).output().unwrap();
        assert!(output.status.success(), "synthetic direct writer failed: {}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn setup() -> Value {
    json!({"history":"explicit", "turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":3_u8}},
    "players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":3_u8,"max":3_u8},"ep":0_u8,"sep":0_u8,
            "zones":{"hand":[{"id":"spell","card":"SYN-01"}],"deck":[{"id":"drawn","card":"SYN-01"},{"id":"remaining","card":"SYN-01"}]}},
        "P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":3_u8,"max":3_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
    }})
}

#[test]
fn pending_wording_and_shared_body_execute_without_hash_proofs() {
    let snapshot = json!({"number":"SYN-01","faces":[{"name":"synthetic","card_type":"スペル","cost":"1","text":"A sentence outside the classifier.","sections":[]}]}).to_string();
    let selected = Catalog::from_current_selection(
        &snapshot,
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        selection(),
    )
    .unwrap();
    let mut game =
        Game::from_selected_effects(selected, &setup(), &Value::Null, &Value::Null, "light")
            .unwrap();
    let result = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(result, CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], json!(2_u8));
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["cemetery"], json!(["spell"]));
}

#[test]
fn unresolved_ruling_only_blocks_games_using_that_card() {
    let spell = json!({"name":"synthetic","card_type":"スペル","cost":"1","text":"Synthetic wording.","sections":[]});
    let snapshot = [
        json!({"number":"SYN-01","faces":[spell]}).to_string(),
        json!({"number":"SYN-02","faces":[spell]}).to_string(),
    ]
    .join("\n");
    let mut input = selection().clone();
    input["authors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"status":"rejected","card_no":"SYN-02","reason":"source_unresolved_issue"}));
    let selected = Catalog::from_current_selection(
        &snapshot,
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        &input,
    )
    .unwrap();
    Game::from_selected_effects(selected, &setup(), &Value::Null, &Value::Null, "unaffected")
        .unwrap();
    let affected_selection = Catalog::from_current_selection(
        &snapshot,
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        &input,
    )
    .unwrap();
    let mut affected = setup();
    affected["players"]["P1"]["zones"]["hand"][0]["card"] = json!("SYN-02");
    Game::from_selected_effects(
        affected_selection,
        &affected,
        &Value::Null,
        &Value::Null,
        "affected",
    )
    .unwrap_err();
}

#[test]
fn direct_handoff_still_rejects_missing_lines_wrong_identity_and_bad_dsl() {
    let lowerer = ExpandedLowerer::new().unwrap();
    for change in ["missing", "owner", "unsupported"] {
        let mut input = selection()["authors"][0]["input"].clone();
        let abilities = &mut input["tree"]["cards"]["SYN-01"]["faces"][FACE]["abilities"];
        match change {
            "missing" => *abilities = json!([]),
            "owner" => {
                abilities[0]["sources"][0]["face_id"] = json!("f:33333333333333333333333333333333");
            }
            _ => abilities[0]["spell"]["do"] = json!([{"unknown_action":{}}]),
        }
        lowerer.lower_current(&input).unwrap_err();
    }
}
