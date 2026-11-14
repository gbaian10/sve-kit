//! Selections from one loader share candidates; printings and changed programs stay checked.

use super::{author, initial, selection};
use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_engine::catalog::{CurrentSelectionLoader, SelectedEffects};
use sve_engine::game::internal::CardPlay;
use sve_engine::game::{Game, View};
use sve_engine::ir::PlayerId;

fn construct(selected: SelectedEffects) -> sve_engine::Result<Game> {
    Game::from_selected_effects(
        selected,
        &initial(0),
        &Value::Null,
        &Value::Null,
        "shared-admission",
    )
}

#[test]
fn a_changed_program_never_reuses_the_admission_of_the_same_card_number() {
    let loader = CurrentSelectionLoader::new().unwrap();
    let (snapshot, keywords, one) = selection(1);
    let (_, _, two) = selection(2);
    // The third selection meets the first candidate again after the changed one was admitted.
    for (input, expected) in [(&one, 1), (&two, 2), (&one, 1)] {
        let selected = loader.select(&snapshot, &keywords, input).unwrap();
        let mut game = construct(selected).unwrap();
        let played = game
            .play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(played, CardPlay::Resolved(_)));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ex"].as_array().map(Vec::len), Some(expected));
    }
}

#[test]
fn a_shared_candidate_is_still_checked_against_every_printing_face() {
    let (creator, mut source) = author(
        "creator",
        "11111111111111111111111111111111",
        "スペル",
        vec![json!({"spell":{"do":[{"draw":{"n":1_u8}}]}})],
    );
    source["printings"] = json!(["creator", "reprint"]);
    let reprint = |kind: &str| {
        let mut card = creator.clone();
        card["number"] = json!("reprint");
        card["faces"][0]["card_type"] = json!(kind);
        card
    };
    let keywords = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let input = json!({"documents":[],"authors":[source]});
    let loader = CurrentSelectionLoader::new().unwrap();
    let select = |kind: &str| {
        let snapshot = format!("{creator}\n{}\n", reprint(kind));
        loader.select(&snapshot, &keywords, &input).unwrap()
    };
    let matching = select("スペル");
    construct(matching.clone()).unwrap();
    let foreign = select("フォロワー");
    assert!(
        Arc::ptr_eq(&matching.current["reprint"], &foreign.current["reprint"]),
        "both selections share the candidate whose program-only checks already passed"
    );
    // The reprint's own face is read again, so the shared outcome cannot admit it.
    construct(foreign).unwrap_err();
    construct(matching).unwrap();
}
