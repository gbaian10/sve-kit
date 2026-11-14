#![expect(
    clippy::indexing_slicing,
    reason = "Synthetic author and snapshot fixtures have complete literal shapes."
)]

use super::{author, rule_fixtures, selection};
use core::array::from_fn;
use serde_json::{Value, json};
use std::sync::OnceLock;
use sve_engine::EngineFailure;
use sve_engine::catalog::{Catalog, CurrentSelectionLoader, EngineIdentityInput, SelectedEffects};
use sve_engine::game::Game;
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{
    AdmissionGap, AdmissionGapReason, DeckConstruction, Input, PlayerDeck, StrictAdmission,
    StrictStartError, Viewer,
};

fn selected(snapshot: &str, keywords: &str, input: &Value) -> SelectedEffects {
    static LOADER: OnceLock<CurrentSelectionLoader> = OnceLock::new();
    LOADER
        .get_or_init(|| CurrentSelectionLoader::new().unwrap())
        .select(snapshot, keywords, input)
        .unwrap()
}

fn decks() -> [PlayerDeck; 2] {
    from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: vec!["creator".into(); 8],
        evolve: vec![],
    })
}

#[expect(
    clippy::panic,
    reason = "An initialization failure is not a deck rejection."
)]
fn rejected(selected: SelectedEffects, decks: [PlayerDeck; 2]) -> StrictAdmission {
    let error = Game::start_player_game(selected, decks, Viewer::P1, "strict-test").unwrap_err();
    let StrictStartError::Admission(report) = error else {
        panic!("expected a deck admission rejection");
    };
    assert!(!report.is_admitted());
    report
}

fn gap(number: &str, reason: AdmissionGapReason) -> AdmissionGap {
    AdmissionGap {
        card_no: number.into(),
        reason,
    }
}

#[test]
fn strict_complete_decks_and_generated_ex_tokens_can_start() {
    let (snapshot, keywords, input) = selection(1);
    let selected = selected(&snapshot, &keywords, &input);
    assert!(
        Game::check_player_decks(&selected, &decks())
            .unwrap()
            .is_admitted()
    );
    let game = Game::start_player_game(selected, decks(), Viewer::P2, "strict-start").unwrap();
    assert!(matches!(game.input(Viewer::P2), Input::Question(_)));
    assert!(matches!(game.input(Viewer::P1), Input::Waiting(_)));
}

#[test]
fn strict_gaps_are_deduplicated_and_only_returned_to_the_deck_owner() {
    let (snapshot, keywords, input) = selection(1);
    let mut decks = decks();
    decks[0]
        .main
        .extend(["missing-one".into(), "missing-one".into()]);
    decks[1].main.push("missing-two".into());
    let report = rejected(selected(&snapshot, &keywords, &input), decks);
    assert_eq!(
        report.gaps(Viewer::P1),
        [gap("missing-one", AdmissionGapReason::MissingSnapshot)]
    );
    assert_eq!(
        report.gaps(Viewer::P2),
        [gap("missing-two", AdmissionGapReason::MissingSnapshot)]
    );
    let debug = format!("{report:?}");
    assert!(!debug.contains("missing-one"));
    assert!(!debug.contains("missing-two"));
}

#[test]
fn strict_known_snapshot_cards_need_a_current_whole_card_program() {
    let (mut snapshot, keywords, input) = selection(1);
    snapshot.push_str(&json!({"number":"not-authored","faces":[{
        "name":"unused","card_type":"フォロワー","cost":"1","power":"1","hp":"1","text":"","sections":[]
    }]}).to_string());
    let mut decks = decks();
    decks[1].main.push("not-authored".into());
    let report = rejected(selected(&snapshot, &keywords, &input), decks);
    assert_eq!(
        report.gaps(Viewer::P2),
        [gap("not-authored", AdmissionGapReason::MissingProgram)]
    );
    assert!(report.gaps(Viewer::P1).is_empty());
}

#[test]
fn strict_checks_evolve_decks_before_starting() {
    let (snapshot, keywords, input) = selection(1);
    let mut decks = decks();
    decks[1].evolve.push("missing-evolve".into());
    let report = rejected(selected(&snapshot, &keywords, &input), decks);
    assert_eq!(
        report.gaps(Viewer::P2),
        [gap("missing-evolve", AdmissionGapReason::MissingSnapshot)]
    );
    assert!(report.gaps(Viewer::P1).is_empty());
}

#[test]
fn strict_rejects_prototype_only_token_dependencies() {
    let (mut snapshot, keywords, mut input) = selection(1);
    input["authors"].as_array_mut().unwrap().truncate(1);
    let (plain, source) = author(
        "plain",
        "33333333333333333333333333333333",
        "フォロワー",
        vec![],
    );
    snapshot.push_str(&plain.to_string());
    input["authors"].as_array_mut().unwrap().push(source);
    input["documents"] = json!([[
        "synthetic",
        "format: 1\nkind: effect_set\ncards:\n  token:\n    status: complete\n    abilities: []\n"
    ]]);
    let mut decks = decks();
    decks[1].main = vec!["plain".into(); 8];
    let report = rejected(selected(&snapshot, &keywords, &input), decks);
    assert_eq!(
        report.gaps(Viewer::P1),
        [gap("token", AdmissionGapReason::MissingProgram)]
    );
    assert!(report.gaps(Viewer::P2).is_empty());
}

#[test]
fn strict_transitive_token_dependencies_are_checked_for_each_owner() {
    let (mut snapshot, keywords, mut input) = selection(1);
    let (token, source) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        vec![
            json!({"fanfare":{"do":[{"create":{"token":"nested","to":"ex","n":1_u8,"result":"made"}}]}}),
        ],
    );
    let (nested, _) = author(
        "nested",
        "33333333333333333333333333333333",
        "フォロワー・トークン",
        vec![],
    );
    snapshot = format!("{}\n{token}\n{nested}\n", snapshot.lines().next().unwrap());
    input["authors"][1] = source;
    input["documents"] = json!([[
        "synthetic",
        "format: 1\nkind: effect_set\ncards:\n  nested:\n    status: complete\n    abilities: []\n"
    ]]);
    let report = rejected(selected(&snapshot, &keywords, &input), decks());
    for owner in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            report.gaps(owner),
            [gap("nested", AdmissionGapReason::MissingProgram)]
        );
    }
}

#[test]
fn strict_runtime_rejects_a_source_accepted_but_unsupported_token() {
    let (snapshot, keywords, input) = selection(1);
    let snapshot = snapshot.replace("フォロワー・トークン", "アミュレット・トークン");
    let report = rejected(selected(&snapshot, &keywords, &input), decks());
    for owner in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            report.gaps(owner),
            [gap("token", AdmissionGapReason::UnsupportedRuntime)]
        );
    }
}

#[test]
fn strict_ignores_unsupported_cards_outside_both_deck_closures() {
    let (mut snapshot, keywords, mut input) = selection(1);
    let (unused, source) = author(
        "unused",
        "33333333333333333333333333333333",
        "アミュレット",
        vec![],
    );
    snapshot.push_str(&unused.to_string());
    input["authors"].as_array_mut().unwrap().push(source);
    let game = Game::start_player_game(
        selected(&snapshot, &keywords, &input),
        decks(),
        Viewer::P1,
        "unused",
    )
    .unwrap();
    assert!(matches!(game.input(Viewer::P1), Input::Question(_)));
}

#[test]
fn strict_unresolved_rulings_are_reported_only_to_the_affected_owner() {
    let (mut snapshot, keywords, mut input) = selection(1);
    let (unresolved, _) = author(
        "unresolved",
        "33333333333333333333333333333333",
        "フォロワー",
        vec![],
    );
    snapshot.push_str(&unresolved.to_string());
    input["authors"].as_array_mut().unwrap().push(json!({
        "status":"rejected","card_no":"unresolved","reason":"source_unresolved_issue"
    }));
    let mut decks = decks();
    decks[0].main.push("unresolved".into());
    let report = rejected(selected(&snapshot, &keywords, &input), decks);
    assert_eq!(
        report.gaps(Viewer::P1),
        [gap("unresolved", AdmissionGapReason::UnresolvedRuling)]
    );
    assert!(report.gaps(Viewer::P2).is_empty());
}

#[test]
fn strict_startup_error_display_does_not_disclose_missing_card_numbers() {
    let (snapshot, keywords, input) = selection(1);
    let mut decks = decks();
    decks[0].evolve.push("not-selected".into());
    let error = Game::start_player_game(
        selected(&snapshot, &keywords, &input),
        decks,
        Viewer::P1,
        "strict-existing",
    )
    .unwrap_err();
    assert!(!error.to_string().contains("not-selected"));
}

#[test]
fn strict_rule_generated_opening_ex_resources_require_current_admission() {
    let (mut plain, source) = author(
        "plain",
        "11111111111111111111111111111111",
        "フォロワー",
        vec![],
    );
    plain["faces"][0]["title"] = json!("synthetic-title");
    let (resource, resource_source) = author(
        "resource",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        vec![],
    );
    let snapshot = format!("{plain}\n{resource}");
    let keywords = "format: 1\nkind: keyword_registry\nkeywords: {}\n";
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([rule_fixtures::title(
            "synthetic_lesson",
            "plain",
            json!([
                {"kind":"opening_ex_resource","resource_role":"lesson_item","count":5_u8,"zone":"ex"}
            ])
        )]),
        json!([rule_fixtures::resource(
            "lesson_item",
            "resource",
            "printed",
            true
        )]),
    );
    let mut decks = decks();
    for deck in &mut decks {
        deck.main = vec!["plain".into(); 8];
    }
    decks[1].construction = DeckConstruction::Title("synthetic_lesson".into());
    let load = |input: &Value| {
        Catalog::from_current_selection_with_rules(
            &snapshot,
            keywords,
            input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap()
    };
    let mut input = json!({"documents":[],"authors":[source,resource_source]});
    let game =
        Game::start_player_game(load(&input), decks.clone(), Viewer::P1, "strict-ex").unwrap();
    let observed = game.observe(Viewer::P2).unwrap();
    let owner = observed
        .players
        .iter()
        .find(|player| player.player == Viewer::P2)
        .unwrap();
    assert!(
        owner
            .zones
            .iter()
            .any(|zone| zone.kind == ObservedZone::Ex && zone.count == 5)
    );
    input["authors"].as_array_mut().unwrap().truncate(1);
    input["documents"] = json!([[
        "synthetic",
        "format: 1\nkind: effect_set\ncards:\n  resource:\n    status: complete\n    abilities: []\n"
    ]]);
    let report = rejected(load(&input), decks);
    assert!(report.gaps(Viewer::P1).is_empty());
    assert_eq!(
        report.gaps(Viewer::P2),
        [gap("resource", AdmissionGapReason::MissingProgram)]
    );
}

#[test]
fn strict_snapshot_face_closure_must_match_current_sources() {
    let (snapshot, keywords, input) = selection(1);
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut card: Value = serde_json::from_str(line).unwrap();
            if card["number"] == "token" {
                let extra = card["faces"][0].clone();
                card["faces"].as_array_mut().unwrap().push(extra);
            }
            card.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let selected = selected(&snapshot, &keywords, &input);
    assert!(matches!(
        Game::check_player_decks(&selected, &decks()),
        Err(EngineFailure::Invalid(_))
    ));
    assert!(matches!(
        Game::start_player_game(selected, decks(), Viewer::P1, "strict-source"),
        Err(StrictStartError::Initialization(EngineFailure::Invalid(_)))
    ));
}

#[test]
fn strict_dependency_gaps_do_not_hide_an_unrelated_unsupported_card() {
    let (mut snapshot, keywords, mut input) = selection(1);
    input["authors"].as_array_mut().unwrap().truncate(1);
    input["documents"] = json!([[
        "synthetic",
        "format: 1\nkind: effect_set\ncards:\n  token:\n    status: complete\n    abilities: []\n"
    ]]);
    let (unsupported, source) = author(
        "unsupported",
        "33333333333333333333333333333333",
        "アミュレット",
        vec![],
    );
    snapshot.push_str(&unsupported.to_string());
    input["authors"].as_array_mut().unwrap().push(source);
    let mut decks = decks();
    decks[0].main.push("unsupported".into());
    let report = rejected(selected(&snapshot, &keywords, &input), decks);
    assert_eq!(
        report.gaps(Viewer::P1),
        [
            gap("token", AdmissionGapReason::MissingProgram),
            gap("unsupported", AdmissionGapReason::UnsupportedRuntime),
        ]
    );
    assert_eq!(
        report.gaps(Viewer::P2),
        [gap("token", AdmissionGapReason::MissingProgram)]
    );
}
