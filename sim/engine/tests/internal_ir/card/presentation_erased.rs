use super::super::player_pipeline::{answer, question};
use super::*;
use sve_engine::game::player::events::{ActionKind, EventKind, MovementMethod, PresentationOp};
use sve_engine::game::player::{DecisionShape, QuestionShape, SubmissionKind, Viewer};

const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

/// An amulet ability that destroys a chosen enemy token and then asks a resolution choice.
/// The destroyed token is erased at once: it stays in the authority with its handle, but it
/// is on nobody's board when the choice is asked.
fn destroys_a_token_then_asks(baseline: bool) -> Game {
    let mut tree = fanfare::expanded::tree().clone();
    let mut template = tree["cards"]["BP01-041"].clone();
    template["abilities"][0]["activate"] = json!({
        "cost":{"pp":1},
        "targets":[{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[
            {"destroy":"victim"},
            {"choose":{"bind":"other","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}},
            {"damage":{"to":"other","n":1}}
        ]
    });
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
    game.load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.advance_player_input();
    if baseline {
        // Each seat is handed its starting board, which shows both tokens.
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
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the first enemy token is a legal target"
    );
    assert!(
        matches!(
            question(&game, Viewer::P1).shape,
            QuestionShape::Decision(DecisionShape::Targets(_))
        ),
        "the resolution choice is asked after the destroyed token was erased"
    );
    game
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
