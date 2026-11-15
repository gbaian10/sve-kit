use super::super::player_pipeline::{answer, question};
use super::*;
use sve_engine::game::player::events::{ActionKind, EventKind, MovementMethod, PresentationOp};
use sve_engine::game::player::{DecisionShape, QuestionShape, SubmissionKind, Viewer};

const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

/// An amulet ability on a board whose followers are all tokens, stopped at the resolution
/// choice that follows `removal`. A token that leaves the field is erased at once: it stays
/// in the authority with its handle, but it is on nobody's board when the choice is asked.
fn stopped_after(removal: &[Value], victim: bool, baseline: bool) -> Game {
    let mut tree = fanfare::expanded::tree().clone();
    let mut template = tree["cards"]["BP01-041"].clone();
    let mut steps = removal.to_vec();
    steps.extend([
        json!({"choose":{"bind":"other","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}}),
        json!({"damage":{"to":"other","n":1}}),
    ]);
    template["abilities"][0]["activate"] = json!({"cost":{"pp":1},"do":steps});
    if victim {
        template["abilities"][0]["activate"]["targets"] = json!([
            {"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}
        ]);
    }
    tree["cards"] = json!({"unit-amulet":template});
    let programs = lowerer().lower(&tree).unwrap();

    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["card_type"] = json!("フォロワー・トークン");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"f","card":"unit-follower"},
        {"id":"g","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "erased-token",
    )
    .unwrap();
    // A created token must already be an admitted card when its creator is loaded.
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
    game.advance_player_input();
    if baseline {
        // Each seat is handed its starting board, which shows both enemy tokens.
        for viewer in SEATS {
            game.present_player_events(viewer).unwrap();
        }
    }
    game.activate_internal_card(
        "s",
        programs["unit-amulet"].card().abilities[0].node.id,
        PlayerId { seat: 1 },
    )
    .unwrap();
    game.advance_player_input();
    if victim {
        assert_eq!(
            answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
            SubmissionKind::Applied,
            "the first enemy token is a legal target"
        );
    }
    assert!(
        matches!(
            question(&game, Viewer::P1).shape,
            QuestionShape::Decision(DecisionShape::Targets(_))
        ),
        "the resolution choice is asked after the removed token was erased"
    );
    game
}

/// Destroys a chosen enemy token that was on the board from the start.
fn destroys_a_token_then_asks(baseline: bool) -> Game {
    stopped_after(&[json!({"destroy":"victim"})], true, baseline)
}

/// Everything this viewer has been shown early, in order.
fn shown(game: &mut Game, viewer: Viewer) -> Vec<EventKind> {
    game.present_player_events(viewer).unwrap();
    game.presentation(viewer, None)
        .unwrap()
        .flat_map(|entry| match &entry.op {
            PresentationOp::Provisional(batch) => batch.events.clone(),
            PresentationOp::Promote { .. } | PresentationOp::Cancel { .. } => Vec::new(),
        })
        .map(|event| event.kind)
        .collect()
}

fn acts(kinds: &[EventKind], wanted: ActionKind) -> bool {
    kinds
        .iter()
        .any(|kind| matches!(kind, EventKind::Action(action) if action.kind == wanted))
}

/// Whether the effect destruction, a move whose method is Destroy, is among these results.
fn destroys(kinds: &[EventKind]) -> bool {
    kinds.iter().any(|kind| {
        matches!(
            kind,
            EventKind::Movement {
                method: MovementMethod::Effect(MoveMethod::Destroy),
                ..
            }
        )
    })
}

fn committed(game: &Game, viewer: Viewer, kind: ActionKind) -> Vec<bool> {
    game.events(viewer, None)
        .unwrap()
        .filter(|event| matches!(&event.kind, EventKind::Action(action) if action.kind == kind))
        .map(|event| event.presentation.is_some())
        .collect()
}

#[test]
fn an_erased_token_never_seen_on_a_handed_board_is_not_shown_early() {
    let mut game = destroys_a_token_then_asks(false);
    for viewer in SEATS {
        let kinds = shown(&mut game, viewer);
        assert!(
            acts(&kinds, ActionKind::AbilityPlay),
            "the played ability names only the amulet, which is on the board now: {kinds:?}"
        );
        assert!(
            !destroys(&kinds) && !acts(&kinds, ActionKind::Erase),
            "the erased token is on no board this seat was handed, although the authority \
             still holds its handle: {kinds:?}"
        );
    }
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the remaining token is chosen and the ability resolves"
    );
    for viewer in SEATS {
        assert_eq!(
            committed(&game, viewer, ActionKind::Erase),
            vec![false],
            "the erasure still commits as ordinary history"
        );
    }
}

#[test]
fn an_erased_token_from_the_previously_handed_board_is_still_shown() {
    let mut game = destroys_a_token_then_asks(true);
    for viewer in SEATS {
        let kinds = shown(&mut game, viewer);
        assert!(
            destroys(&kinds) && acts(&kinds, ActionKind::Erase),
            "the seat saw this token on its previous board, so its destruction and erasure \
             are shown before the choice: {kinds:?}"
        );
    }
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the remaining token is chosen and the ability resolves"
    );
    for viewer in SEATS {
        assert_eq!(
            committed(&game, viewer, ActionKind::Erase),
            vec![true],
            "the shown erasure is promoted"
        );
    }
}

#[test]
fn a_token_created_and_erased_before_the_stop_is_not_shown_early() {
    // The ability creates a token and then destroys every follower its controller has, which
    // is only that token. Both seats were handed the starting board, but the token never
    // appeared on any handed board.
    let mut game = stopped_after(
        &[
            json!({"create":{"token":"unit-follower","to":"field","n":1}}),
            json!({"destroy":{"ref":{"who":"self","zone":"field","kind":"follower"}}}),
        ],
        false,
        true,
    );
    for viewer in SEATS {
        let kinds = shown(&mut game, viewer);
        assert!(
            acts(&kinds, ActionKind::AbilityPlay),
            "the played ability is shown: {kinds:?}"
        );
        assert!(
            !destroys(&kinds)
                && !acts(&kinds, ActionKind::Erase)
                && !kinds
                    .iter()
                    .any(|kind| matches!(kind, EventKind::Movement { .. })),
            "nothing about the token that came and went inside this resolution is shown \
             before it commits: {kinds:?}"
        );
    }
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "an enemy token is chosen and the ability resolves"
    );
    for viewer in SEATS {
        assert_eq!(
            committed(&game, viewer, ActionKind::Erase),
            vec![false],
            "the token's erasure commits as ordinary history"
        );
    }
}
