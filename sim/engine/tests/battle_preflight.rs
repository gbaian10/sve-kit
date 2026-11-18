//! Match-wide admission precedes opening and uses the live source resolver.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "Malformed synthetic fixtures and unexpected public questions fail immediately."
)]

#[path = "support/current_fixture.rs"]
mod current_fixture;
#[path = "support/rule_fixtures.rs"]
mod rule_fixtures;

use core::array::from_fn;
use serde_json::json;
use std::sync::OnceLock;
use sve_battle_contract::{
    Answer, BattleInput, CardRef, Nat, QuestionId, Seat, SourceRef, question as wire,
};
use sve_engine::battle::{
    BattleReferences, InputProjection, PrintingFaceIdentity, ReferenceInput, RuleIdentity,
    RuleInput, WordingFaceIdentity, WordingFieldIdentity, WordingInput, WordingRevisionIdentity,
};
use sve_engine::catalog::{CurrentSelectionLoader, EngineIdentityInput, SelectedEffects};
use sve_engine::game::Game;
use sve_engine::game::player::{
    AdmissionGapReason, DeckConstruction, PlayerDeck, SubmissionKind, Viewer,
};

struct Fixture {
    selected: SelectedEffects,
    input: ReferenceInput,
}

fn loader() -> &'static CurrentSelectionLoader {
    static LOADER: OnceLock<CurrentSelectionLoader> = OnceLock::new();
    LOADER.get_or_init(|| CurrentSelectionLoader::new().unwrap())
}

fn fixture(creator: bool, token_kind: &str) -> Fixture {
    let steps = if creator {
        json!([{"if":{"cond":{"my_turn":true},"then":[
            {"create":{"token":"token","to":"ex","n":1_u8,"result":"made"}}
        ]}}])
    } else {
        json!([{"draw":{"n":0_u8}}])
    };
    let entries = [
        (
            "unit-follower",
            "11111111111111111111111111111111",
            "フォロワー",
            vec![json!({"activate":{"cost":{"pp":0_u8},"do":steps}})],
        ),
        (
            "plain",
            "22222222222222222222222222222222",
            "フォロワー",
            vec![],
        ),
        (
            "token",
            "33333333333333333333333333333333",
            token_kind,
            vec![json!({"activate":{"cost":{"pp":0_u8},"do":[{"draw":{"n":0_u8}}]}})],
        ),
    ];
    let mut snapshot = Vec::new();
    let mut authors = Vec::new();
    let mut faces = Vec::new();
    let mut wording = Vec::new();
    for (number, id, kind, abilities) in entries {
        let (mut card, author) = current_fixture::author(number, id, kind, abilities);
        if number == "plain" {
            card["faces"][0]["title"] = json!("synthetic-title");
        }
        snapshot.push(card.to_string());
        authors.push(author);
        faces.push(PrintingFaceIdentity {
            card_no: number.into(),
            native_face: 0,
            card: CardRef {
                printing_id: format!("printing-{number}"),
                face: Nat::ZERO,
            },
        });
        wording.push(WordingFaceIdentity {
            card_no: number.into(),
            native_face: 0,
            face_id: format!("f:{id}"),
            revisions: vec![WordingRevisionIdentity {
                revision_id: format!("revision-{number}"),
                fields: vec![WordingFieldIdentity::Effect {}],
            }],
        });
    }
    let snapshot = snapshot.join("\n");
    let keywords = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([rule_fixtures::title("synthetic", "plain", json!([]))]),
        json!([]),
    );
    let selected = loader()
        .select_with_rules(
            &snapshot,
            &keywords,
            &json!({"documents":[],"authors":authors}),
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
    let manifest = format!("sha256:{}", "a".repeat(64));
    Fixture {
        selected,
        input: ReferenceInput {
            region: "jp".into(),
            snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
            faces,
            wording: Some(WordingInput {
                manifest_sha256: manifest.clone(),
                faces: wording,
            }),
            rules: Some(RuleInput {
                manifest_sha256: manifest,
                entries: vec![RuleIdentity::Cr {
                    id: "1.2.3".into(),
                    revision: "exact-cr-revision".into(),
                }],
            }),
        },
    }
}

fn decks() -> [PlayerDeck; 2] {
    from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: vec!["unit-follower".into(); 8],
        evolve: vec![],
    })
}
fn project(
    game: &Game,
    viewer: Viewer,
    id: &str,
    references: &BattleReferences,
) -> InputProjection {
    game.battle_input(viewer, &QuestionId::new(id.into()), references)
        .unwrap()
}
fn question(input: &InputProjection) -> &wire::Question {
    let BattleInput::Question { question } = input.input() else {
        panic!("public question")
    };
    question
}
fn submit(game: &mut Game, viewer: Viewer, input: &InputProjection, answer: &Answer) {
    let command = input.answer(viewer, &question(input).id, answer).unwrap();
    assert_eq!(
        game.submit_player(viewer, command).kind,
        SubmissionKind::Applied,
        "the admitted wire answer must advance the authority"
    );
}

#[test]
fn preflight_starts_a_real_wire_game_with_exact_text_but_no_invented_complete_rules() {
    let fixture = fixture(false, "フォロワー・トークン");
    let references = BattleReferences::new(&fixture.selected.prototypes, fixture.input).unwrap();
    assert!(
        Game::check_battle_decks(&fixture.selected, &decks(), &references)
            .unwrap()
            .is_admitted()
    );
    let mut game =
        Game::start_player_game(fixture.selected, decks(), Viewer::P1, "preflight-opening")
            .unwrap();
    let first = project(&game, Viewer::P1, "first", &references);
    assert_eq!(
        question(&first).rules,
        None,
        "a located single CR clause is not a complete question basis"
    );
    let wire::QuestionShape::Choose { options, .. } = &question(&first).shape else {
        panic!("first player")
    };
    let chosen = options
        .iter()
        .find(|option| option.value == wire::ChoiceValue::Seat { player: Seat::P1 })
        .unwrap()
        .id
        .clone();
    submit(
        &mut game,
        Viewer::P1,
        &first,
        &Answer::Choose {
            chosen: vec![chosen],
        },
    );
    for (viewer, id) in [(Viewer::P1, "keep-first"), (Viewer::P2, "keep-second")] {
        let input = project(&game, viewer, id, &references);
        submit(
            &mut game,
            viewer,
            &input,
            &Answer::Mulligan {
                redraw: false,
                bottom_order: vec![],
            },
        );
    }
    let play_input = project(&game, Viewer::P1, "play", &references);
    let wire::QuestionShape::ActionWindow {
        intents: play_intents,
    } = &question(&play_input).shape
    else {
        panic!("play")
    };
    let play_id = play_intents
        .iter()
        .find_map(|intent| {
            if let wire::MainIntent::Play { id, .. } = intent {
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap();
    submit(
        &mut game,
        Viewer::P1,
        &play_input,
        &Answer::ActionWindow {
            intent: play_id,
            target: None,
        },
    );
    let input = project(&game, Viewer::P1, "activate", &references);
    let wire::QuestionShape::ActionWindow { intents } = &question(&input).shape else {
        panic!("activation")
    };
    let (id, ability) = intents
        .iter()
        .find_map(|intent| {
            if let wire::MainIntent::Activate { id, ability, .. } = intent {
                Some((id.clone(), ability))
            } else {
                None
            }
        })
        .unwrap();
    let SourceRef::Text { text } = ability else {
        panic!("exact admitted wording")
    };
    assert_eq!(text.wording_revision, "revision-unit-follower");
    assert_eq!(text.card.printing_id, "printing-unit-follower");
    submit(
        &mut game,
        Viewer::P1,
        &input,
        &Answer::ActionWindow {
            intent: id,
            target: None,
        },
    );
}

#[test]
fn preflight_rejects_the_unsupported_dependency_inside_an_unexecuted_branch_for_its_owner() {
    let fixture = fixture(true, "アミュレット・トークン");
    let references = BattleReferences::new(&fixture.selected.prototypes, fixture.input).unwrap();
    for owner in [Viewer::P1, Viewer::P2] {
        let mut decks = decks();
        let other = usize::from(owner == Viewer::P1);
        decks[other].main = vec!["plain".into(); 8];
        let report = Game::check_battle_decks(&fixture.selected, &decks, &references).unwrap();
        assert!(!report.is_admitted());
        assert!(
            report.gaps(owner).iter().any(|gap| gap.card_no == "token"
                && gap.reason == AdmissionGapReason::UnsupportedRuntime)
        );
        assert_eq!(
            report.gaps(if owner == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            }),
            []
        );
    }
}

#[test]
fn preflight_checks_exact_wording_in_the_transitive_closure_without_testing_the_branch() {
    let fixture = fixture(true, "フォロワー・トークン");
    let mut input = fixture.input;
    input
        .wording
        .as_mut()
        .unwrap()
        .faces
        .iter_mut()
        .find(|face| face.card_no == "token")
        .unwrap()
        .revisions
        .clear();
    let references = BattleReferences::new(&fixture.selected.prototypes, input).unwrap();
    assert!(
        Game::check_player_decks(&fixture.selected, &decks())
            .unwrap()
            .is_admitted()
    );
    let failure = Game::check_battle_decks(&fixture.selected, &decks(), &references).unwrap_err();
    assert!(
        !failure.to_string().contains("token"),
        "host failures do not name an opponent's hidden dependency"
    );
    let mut unrelated = decks();
    for deck in &mut unrelated {
        deck.main = vec!["plain".into(); 8];
    }
    assert!(
        Game::check_battle_decks(&fixture.selected, &unrelated, &references)
            .unwrap()
            .is_admitted(),
        "unreachable authors do not belong to this match's program closure"
    );
}
