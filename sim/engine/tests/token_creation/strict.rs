use super::{author, selection};
use core::array::from_fn;
use serde_json::{Value, json};
use std::sync::OnceLock;
use sve_engine::EngineFailure;
use sve_engine::catalog::{CurrentSelectionLoader, SelectedEffects};
use sve_engine::game::Game;
use sve_engine::game::player::{
    AdmissionGap, AdmissionGapReason, DeckConstruction, PlayerDeck, StrictAdmission,
    StrictStartError, Viewer,
};

fn loader() -> &'static CurrentSelectionLoader {
    static LOADER: OnceLock<CurrentSelectionLoader> = OnceLock::new();
    LOADER.get_or_init(|| CurrentSelectionLoader::new().unwrap())
}

fn selected(snapshot: &str, keywords: &str, input: &Value) -> SelectedEffects {
    loader().select(snapshot, keywords, input).unwrap()
}

fn reachable(
    snapshot: &str,
    keywords: &str,
    input: &Value,
    decks: &[PlayerDeck; 2],
) -> sve_engine::Result<SelectedEffects> {
    Game::select_player_decks(loader(), snapshot, keywords, input, None, decks)
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
    assert!(
        !report.is_admitted(),
        "An admission rejection must contain a missing requirement"
    );
    report
}

fn gap(number: &str, reason: AdmissionGapReason) -> AdmissionGap {
    AdmissionGap {
        card_no: number.into(),
        reason,
    }
}

#[test]
fn deck_scoped_selection_keeps_gaps_and_acceptance_checks() {
    let (mut snapshot, keywords, input) = selection(1);
    snapshot.push_str(&json!({"number":"not-authored","faces":[{
        "name":"unused","card_type":"フォロワー","cost":"1","power":"1","hp":"1","text":"","sections":[]
    }]}).to_string());
    let mut decks = decks();
    decks[0].main.push("missing-printing".into());
    decks[1].main.push("not-authored".into());
    let scoped = reachable(&snapshot, &keywords, &input, &decks).unwrap();
    let report = rejected(scoped.clone(), decks.clone());
    assert_eq!(
        report.gaps(Viewer::P1),
        [gap("missing-printing", AdmissionGapReason::MissingSnapshot)]
    );
    assert_eq!(
        report.gaps(Viewer::P2),
        [gap("not-authored", AdmissionGapReason::MissingProgram)]
    );
    assert_eq!(
        report,
        rejected(selected(&snapshot, &keywords, &input), decks.clone())
    );
    let mut changed = scoped;
    changed.current.clear();
    assert!(matches!(
        Game::check_player_decks(&changed, &decks),
        Err(EngineFailure::Invalid(_))
    ));
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
    assert_eq!(report.gaps(Viewer::P2), []);
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
    assert_eq!(report.gaps(Viewer::P2), []);
}
