#![expect(
    clippy::panic,
    reason = "The real-card protocol must expose the expected typed question."
)]

use super::*;
use sve_engine::game::player::events::{
    ActionKind, EventError, EventKind, MovementMethod, PresentationCursor, PresentationOp,
    ProvisionalBatch, ScopeId,
};
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{
    Command, DecisionShape, DiscardKind, Input, MainActionAnswer, MainIntentKind, Question,
    QuestionShape, SubmissionKind, Viewer,
};

pub(super) const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

pub(super) fn question(game: &Game) -> Question {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("player question");
    };
    *question
}

/// P1 holds `card` and one other card; the deck has enough cards to draw.
pub(super) fn position(card: &str, adjust: impl FnOnce(&mut Value)) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":card}, {"id":"kept","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw0","card":"BP01-171"}, {"id":"draw1","card":"BP01-171"},
        {"id":"tail","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy1","card":"BP01-037"}, {"id":"enemy2","card":"BP01-037"}
    ]);
    adjust(&mut initial);
    let mut game = game(&initial);
    game.advance_player_input();
    game
}

pub(super) fn declare(game: &mut Game, spell: &str) {
    let main = question(game);
    let QuestionShape::ActionWindow(actions) = &main.shape else {
        panic!("main declarations");
    };
    let play = actions
        .intents
        .iter()
        .find(|intent| matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == spell))
        .unwrap();
    let submission = game.submit_player(
        Viewer::P1,
        Command::MainAction {
            token: main.token,
            intent: play.id.clone(),
            answer: MainActionAnswer::None,
        },
    );
    assert_eq!(
        submission.kind,
        SubmissionKind::Applied,
        "the public intent starts the real card declaration"
    );
}

pub(super) fn answer(game: &mut Game, answer: Answer) -> SubmissionKind {
    let token = question(game).token;
    game.submit_player(Viewer::P1, Command::Answer { token, answer })
        .kind
}

/// Every step recorded for one viewer after a cursor, as a host reads it.
pub(super) fn recorded(
    game: &Game,
    viewer: Viewer,
    after: Option<&PresentationCursor>,
) -> (Vec<PresentationOp>, Option<PresentationCursor>) {
    let entries = game
        .presentation(viewer, after)
        .unwrap()
        .cloned()
        .collect::<Vec<_>>();
    let cursor = entries.last().map(|entry| entry.cursor.clone());
    let steps = entries.into_iter().map(|entry| entry.op).collect();
    (steps, cursor.or_else(|| after.cloned()))
}

/// The batches opened by these steps, in delivery order.
pub(super) fn shown(steps: &[PresentationOp]) -> Vec<ProvisionalBatch> {
    steps
        .iter()
        .filter_map(|step| match step {
            PresentationOp::Provisional(batch) => Some(batch.clone()),
            PresentationOp::Promote { .. } => None,
        })
        .collect()
}

/// The scopes these steps turned into history, in delivery order.
pub(super) fn promoted(steps: &[PresentationOp]) -> Vec<ScopeId> {
    steps
        .iter()
        .filter_map(|step| match step {
            PresentationOp::Promote { scope } => Some(scope.clone()),
            PresentationOp::Provisional(_) => None,
        })
        .collect()
}

pub(super) fn kinds(batches: &[ProvisionalBatch]) -> Vec<&EventKind> {
    batches
        .iter()
        .flat_map(|batch| batch.events.iter().map(|event| &event.kind))
        .collect()
}

fn position_of(kinds: &[&EventKind], wanted: impl Fn(&EventKind) -> bool) -> usize {
    let found = kinds
        .iter()
        .enumerate()
        .filter(|(_, kind)| wanted(kind))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "exactly one such occurrence: {kinds:?}");
    found[0]
}

pub(super) fn is_action(kind: &EventKind, wanted: ActionKind) -> bool {
    matches!(kind, EventKind::Action(action) if action.kind == wanted)
}

fn enters_resolution(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Movement { after, .. } if after.zone == Some(ObservedZone::Resolution))
}

const fn is_draw(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Movement {
            method: MovementMethod::Draw,
            ..
        }
    )
}

/// Ask for a presentation on behalf of both seats and return what each newly received.
pub(super) fn present(
    game: &mut Game,
    cursors: &mut [Option<PresentationCursor>; 2],
) -> [Vec<ProvisionalBatch>; 2] {
    SEATS.map(|viewer| {
        let slot = usize::from(viewer == Viewer::P2);
        game.present_player_events(viewer).unwrap();
        let (steps, next) = recorded(game, viewer, cursors[slot].as_ref());
        cursors[slot] = next;
        shown(&steps)
    })
}

#[test]
fn presentation_shows_each_completed_batch_once_before_the_next_question() {
    let mut game = position("BP09-058", |_| {});
    let mut cursors = [None, None];
    declare(&mut game, "BP09-058");
    assert!(
        matches!(
            question(&game).shape,
            QuestionShape::Decision(DecisionShape::Targets(_))
        ),
        "the printed selection is asked during the resolution, after the spell is played"
    );
    for (viewer, batches) in SEATS.into_iter().zip(present(&mut game, &mut cursors)) {
        let kinds = kinds(&batches);
        let resolution = position_of(&kinds, enters_resolution);
        let played = position_of(&kinds, |kind| is_action(kind, ActionKind::Play));
        assert!(
            resolution < played && kinds.len() == 2,
            "the first qualifying stop shows the declaration and the play in rules order: {kinds:?}"
        );
        assert_eq!(batches.len(), 2, "sequential results are separate batches");
        assert_ne!(
            batches[0].scope, batches[1].scope,
            "every complete batch opens its own scope"
        );
        assert_eq!(
            game.events(viewer, None).unwrap().count(),
            0,
            "showing a batch early commits nothing"
        );
    }

    assert_eq!(
        answer(&mut game, Answer::Targets(vec![0])),
        SubmissionKind::Applied,
        "the only enemy follower is a legal selection"
    );
    assert!(
        matches!(
            question(&game).shape,
            QuestionShape::Decision(DecisionShape::Discard {
                kind: DiscardKind::Effect,
                ..
            })
        ),
        "damage and the draw are done; the discard is the next real stop"
    );
    let shown = present(&mut game, &mut cursors);
    for (viewer, batches) in SEATS.into_iter().zip(&shown) {
        let kinds = kinds(batches);
        let damage = position_of(&kinds, |kind| is_action(kind, ActionKind::Damage));
        let drawn = position_of(&kinds, is_draw);
        assert!(
            damage < drawn && kinds.len() == 2,
            "only the results completed since the last stop are new: {kinds:?}"
        );
        let EventKind::Movement { before, after, .. } = kinds[drawn] else {
            panic!("draw movement");
        };
        assert_eq!(
            after.card.is_some(),
            viewer == Viewer::P1,
            "each seat is shown the draw with its own permissions"
        );
        assert!(
            before.card.is_none(),
            "the deck top was unknown to both seats"
        );
        assert_eq!(
            game.events(viewer, None).unwrap().count(),
            0,
            "the resolution is still inside its origin"
        );
    }

    let again = present(&mut game, &mut cursors);
    assert_eq!(again, [vec![], vec![]], "a batch is shown to a viewer once");
    for viewer in SEATS {
        assert_eq!(
            recorded(&game, viewer, None).0.len(),
            4,
            "the record keeps every delivered step in order"
        );
    }
    assert!(
        matches!(
            game.presentation(Viewer::P2, cursors[0].as_ref()),
            Err(EventError::InvalidCursor)
        ),
        "a cursor belongs to one viewer"
    );
}

#[test]
fn presentation_shows_nothing_of_a_declared_card_until_it_is_played() {
    for pp in [9_u8, 10] {
        let mut game = position("BP04-066", |initial| {
            initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
        });
        let mut cursors = [None, None];
        declare(&mut game, "BP04-066");
        let resolution = |current: &Game| {
            current.observe(Viewer::P2).unwrap().players[0]
                .zones
                .iter()
                .find(|zone| zone.kind == ObservedZone::Resolution)
                .unwrap()
                .count
        };
        for (shape, reply) in [
            ("surcharge", Answer::OptionalCost(true)),
            ("targets", Answer::Targets(vec![0])),
        ] {
            assert_eq!(
                resolution(&game),
                1,
                "the declared card already moved, so an uncommitted result exists at the {shape} question"
            );
            assert_eq!(
                present(&mut game, &mut cursors),
                [vec![], vec![]],
                "payment has not happened: the whole declaration can still return to its origin"
            );
            let submitted = answer(&mut game, reply);
            let expected = if (shape, pp) == ("targets", 9) {
                SubmissionKind::Reverted
            } else {
                SubmissionKind::Applied
            };
            assert_eq!(
                submitted, expected,
                "nine PP cannot pay the accepted surcharge after the {shape} answer"
            );
        }
        assert_eq!(
            present(&mut game, &mut cursors),
            [vec![], vec![]],
            "no typed execution is suspended any more"
        );
        for viewer in SEATS {
            assert_eq!(
                game.presentation(viewer, None).unwrap().count(),
                0,
                "a play that never stopped after being played commits directly, and a reverted one shows nothing"
            );
            assert_eq!(
                game.events(viewer, None).unwrap().count() > 0,
                pp == 10,
                "only the completed play becomes committed history"
            );
        }
    }
}

#[test]
fn presentation_includes_the_paid_cost_at_the_first_stop_after_the_play() {
    let mut game = position("BP04-110", |initial| {
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"a","card":"BP19-108"},{"id":"b","card":"BP19-108"}]);
    });
    let mut cursors = [None, None];
    declare(&mut game, "BP04-110");
    let mut stops = Vec::new();
    loop {
        let shape = question(&game).shape;
        let reply = if let QuestionShape::Decision(DecisionShape::Pending(options)) = &shape {
            stops.push("pending");
            Answer::Pending(options[0].handle)
        } else if matches!(shape, QuestionShape::Decision(DecisionShape::OptionalCost)) {
            stops.push("optional");
            Answer::OptionalCost(true)
        } else if let QuestionShape::Decision(DecisionShape::CostMaterials(materials)) = &shape {
            stops.push("materials");
            Answer::CostMaterials(
                materials
                    .members
                    .iter()
                    .map(|member| member.handle)
                    .collect(),
            )
        } else {
            assert!(
                matches!(
                    shape,
                    QuestionShape::Decision(DecisionShape::Discard {
                        kind: DiscardKind::Effect,
                        ..
                    })
                ),
                "unexpected question {shape:?}"
            );
            break;
        };
        assert_eq!(
            present(&mut game, &mut cursors),
            [vec![], vec![]],
            "before the Fanfare is played its cost can still be refused or fail: {stops:?}"
        );
        assert_eq!(
            answer(&mut game, reply),
            SubmissionKind::Applied,
            "each declaration answer is legal: {stops:?}"
        );
    }
    assert!(
        stops.contains(&"materials"),
        "the head cost was a real question before the play: {stops:?}"
    );
    let committed = game.events(Viewer::P1, None).unwrap().count();
    for batches in present(&mut game, &mut cursors) {
        let kinds = kinds(&batches);
        let acted = kinds
            .iter()
            .enumerate()
            .filter(|(_, kind)| is_action(kind, ActionKind::Act))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let played = position_of(&kinds, |kind| is_action(kind, ActionKind::AbilityPlay));
        let drawn = kinds
            .iter()
            .enumerate()
            .filter(|(_, kind)| is_draw(kind))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!((acted.len(), drawn.len()), (2, 2), "{kinds:?}");
        assert!(
            acted.iter().all(|index| *index < played) && drawn.iter().all(|index| played < *index),
            "the first qualifying stop shows cost, play and completed effects in rules order: {kinds:?}"
        );
    }
    assert_eq!(
        game.events(Viewer::P1, None).unwrap().count(),
        committed,
        "the earlier card play stays committed history; the Fanfare adds none yet"
    );
}

#[test]
fn presentation_shows_only_what_a_viewer_asked_for_and_a_rejection_changes_nothing() {
    let mut game = position("BP08-045", |_| {});
    declare(&mut game, "BP08-045");
    assert!(
        matches!(
            question(&game).shape,
            QuestionShape::Decision(DecisionShape::Discard { .. })
        ),
        "the spell is played and both draws are done before the discard is asked"
    );
    game.present_player_events(Viewer::P1).unwrap();
    let (owner, cursor) = recorded(&game, Viewer::P1, None);
    let batches = shown(&owner);
    let kinds = kinds(&batches);
    assert_eq!(
        kinds.iter().filter(|kind| is_draw(kind)).count(),
        2,
        "both completed draws are shown before the discard is asked: {kinds:?}"
    );
    assert_eq!(
        recorded(&game, Viewer::P2, None).0,
        vec![],
        "a seat that was not handed an update has no scope"
    );

    let stale = question(&game).token;
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: stale,
                answer: Answer::Targets(vec![]),
            },
        )
        .kind,
        SubmissionKind::Rejected,
        "an empty selection is outside the discard's domain"
    );
    game.present_player_events(Viewer::P1).unwrap();
    assert_eq!(
        recorded(&game, Viewer::P1, cursor.as_ref()).0,
        vec![],
        "a rejected answer produces no new batch and replays none"
    );
    assert_eq!(
        recorded(&game, Viewer::P1, None).0,
        owner,
        "rereading from the start returns the same prefix"
    );
}
