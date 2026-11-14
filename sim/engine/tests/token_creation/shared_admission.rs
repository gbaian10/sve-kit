//! Selections from one loader share candidates; printings and changed programs stay checked.

use super::{author, initial, selection};
use alloc::sync::Arc;
use core::array::from_fn;
use serde_json::{Value, json};
use sve_engine::catalog::{CurrentSelectionLoader, SelectedEffects};
use sve_engine::game::internal::CardPlay;
use sve_engine::game::player::{
    AdmissionGap, AdmissionGapReason, DeckConstruction, PlayerDeck, Viewer,
};
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

#[test]
fn a_program_rejection_is_identical_for_every_printing_and_selection_until_the_program_changes() {
    let (plain, keywords, mut admitted) = selection(1);
    let mut reprint: Value = serde_json::from_str(plain.lines().next().unwrap()).unwrap();
    reprint["number"] = json!("reprint");
    let snapshot = format!("{plain}{reprint}\n");
    admitted["authors"][0]["printings"] = json!(["creator", "reprint"]);
    let stages = |extra: Option<Value>| {
        let mut stages = vec![json!([{"banish":{"top":{"who":"p","n":2_u8}}}])];
        stages.extend(extra);
        json!([
            {"banish":{"ref":{"any_of":[{"who":"all","zone":"field"},{"who":"all","zone":"ex"}]}}},
            {"each_player":{"as":"p","stages":stages}}
        ])
    };
    let mut rejected = admitted.clone();
    for (input, extra) in [
        (&mut admitted, None),
        // Each player's deck top stays private, so a further stage that draws is refused at load.
        (&mut rejected, Some(json!([{"draw":{"n":1_u8}}]))),
    ] {
        input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
            ["abilities"][0]["spell"]["do"] = stages(extra);
    }
    let failure = |loader: &CurrentSelectionLoader, input: &Value| {
        let selected = loader.select(&snapshot, &keywords, input).unwrap();
        construct(selected).unwrap_err().to_string()
    };
    let unshared = failure(&CurrentSelectionLoader::new().unwrap(), &rejected);
    assert!(unshared.starts_with("unsupported: "), "{unshared}");
    let loader = CurrentSelectionLoader::new().unwrap();
    assert_eq!(failure(&loader, &rejected), unshared);
    assert_eq!(failure(&loader, &rejected), unshared);

    // Each printing of the rejected candidate is its own strict gap.
    let decks: [PlayerDeck; 2] = from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: ["creator", "reprint"]
            .into_iter()
            .cycle()
            .take(8)
            .map(str::to_owned)
            .collect(),
        evolve: vec![],
    });
    let gap = |number: &str| AdmissionGap {
        card_no: number.into(),
        reason: AdmissionGapReason::UnsupportedRuntime,
    };
    let selected = loader.select(&snapshot, &keywords, &rejected).unwrap();
    let report = Game::check_player_decks(&selected, &decks).unwrap();
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(report.gaps(viewer), [gap("creator"), gap("reprint")]);
    }

    // The corrected program is another candidate; the rejected one stays rejected beside it.
    let corrected = loader.select(&snapshot, &keywords, &admitted).unwrap();
    assert!(
        Game::check_player_decks(&corrected, &decks)
            .unwrap()
            .is_admitted()
    );
    construct(corrected).unwrap();
    assert_eq!(failure(&loader, &rejected), unshared);
}
