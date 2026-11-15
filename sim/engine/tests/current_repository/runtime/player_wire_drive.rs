#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Each check reads a few of the many wire event bodies."
)]

use super::drive::{drive_game, position};
use super::player_presentation::{SEATS, question};
use super::player_presentation_drive_fault::reach_the_drive_question;
use super::*;
use sve_battle_contract::event::{self as wire, EventBody};
use sve_battle_contract::observation::TurnView;
use sve_battle_contract::{
    CardRef, Nat as WireNat, ObjectHandle as WireHandle, Seat, SourceRef, Subject,
};
use sve_engine::battle::{EventObjects, ProjectionError, project_batch};
use sve_engine::game::player::events::EventCard;
use sve_engine::game::player::observation::{EffectSource, TurnObservation};
use sve_engine::game::player::{Command, DecisionShape, Input, QuestionShape, SubmissionKind, Viewer};

/// Stands in for the board projection: a native card number is its printing and a native
/// handle is reused as the wire object.
struct Faces;

impl EventObjects for Faces {
    fn card(&self, card: &EventCard) -> Result<wire::EventCard, ProjectionError> {
        Ok(wire::EventCard {
            card: CardRef {
                printing_id: card.properties.card.clone(),
                face: WireNat::try_from(card.properties.face)?,
            },
            object: card
                .handle
                .as_ref()
                .map(|handle| WireHandle::new(handle.as_str().to_owned())),
        })
    }

    fn source(&self, source: &EffectSource) -> Result<SourceRef, ProjectionError> {
        Ok(SourceRef::Card {
            printing_id: source.card.clone(),
            face: None,
            line: source.line.map(WireNat::try_from).transpose()?,
            holder: None,
        })
    }

    fn turn(&self, turn: &TurnObservation) -> Result<TurnView, ProjectionError> {
        let [first, second] = turn.elapsed_turns;
        Ok(TurnView {
            active: turn.active.map(|viewer| match viewer {
                Viewer::P1 => Seat::P1,
                Viewer::P2 => Seat::P2,
            }),
            first_player: None,
            phase: None,
            elapsed_turns: [WireNat::try_from(first)?, WireNat::try_from(second)?],
        })
    }
}

/// CP03-026 attacks the opposing leader and checks twice. Every card it can check has
/// verified icon facts: the first has a draw icon, the others have none. P1 answers the
/// trigger question with `accept` and both seats pass every Quick window.
fn twin_drive_resolved(accept: bool) -> Game {
    let mut initial = position("CP03-025", "CP03-026", 5);
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
    let facts = json!({
        "BP01-037":{"verified":true,"trigger_icon":"draw"},
        "BP01-006":{"verified":true,"trigger_icon":"none"},
        "BP01-009":{"verified":true,"trigger_icon":"none"}
    });
    let mut game = drive_game(&initial, &facts);
    game.advance_player_input();
    reach_the_drive_question(&mut game, &mut [None, None]);
    let token = question(&game).token;
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token,
                answer: Answer::DriveTrigger {
                    accept,
                    target: None,
                },
            },
        )
        .kind,
        SubmissionKind::Applied,
        "the trigger answer is legal and both checks complete"
    );
    while let Some((viewer, window)) = SEATS.into_iter().find_map(|viewer| match game.input(viewer) {
        Input::Question(current)
            if matches!(
                current.shape,
                QuestionShape::Decision(DecisionShape::Quick { .. })
            ) =>
        {
            Some((viewer, current.token))
        }
        Input::Question(_) | Input::Waiting(_) | Input::Finished { .. } | Input::EngineFault => {
            None
        }
    }) {
        assert_eq!(
            game.submit_player(
                viewer,
                Command::Answer {
                    token: window,
                    answer: Answer::Quick(None),
                },
            )
            .kind,
            SubmissionKind::Applied,
            "passing a Quick window is legal"
        );
    }
    game
}

/// Every committed wire body of this viewer, in order.
fn bodies(game: &Game, viewer: Viewer) -> Vec<EventBody> {
    game.event_batches(viewer, None)
        .unwrap()
        .flat_map(|batch| {
            project_batch(batch.events.iter().map(|event| &event.kind), &Faces).unwrap()
        })
        .map(|event| event.body)
        .collect()
}

/// What each Drive result says: its kind, the checked printing, and whether it was accepted
/// with where the card went, or the icon that triggered.
fn drive_results(bodies: &[EventBody]) -> Vec<String> {
    bodies
        .iter()
        .filter_map(|body| match body {
            EventBody::DriveTrigger {
                player: Seat::P1,
                card: Some(card),
                icon,
            } => Some(format!(
                "trigger {} {icon:?} linked={}",
                card.card.printing_id,
                card.object.is_some()
            )),
            EventBody::DriveChecked {
                player: Seat::P1,
                card: Some(card),
                accepted,
                destination,
            } => Some(format!(
                "checked {} accepted={accepted} to {:?} of {:?}",
                card.card.printing_id, destination.zone, destination.player
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn a_twin_drive_states_each_check_its_icon_whether_it_was_accepted_and_where_the_card_went() {
    for (accept, expected) in [
        (
            false,
            vec![
                "checked BP01-037 accepted=false to Deck of P1",
                "checked BP01-006 accepted=false to Deck of P1",
            ],
        ),
        (
            true,
            vec![
                "trigger BP01-037 Draw linked=true",
                "checked BP01-037 accepted=true to Cemetery of P1",
                "checked BP01-009 accepted=false to Deck of P1",
            ],
        ),
    ] {
        let game = twin_drive_resolved(accept);
        for viewer in SEATS {
            let bodies = bodies(&game, viewer);
            assert_eq!(
                drive_results(&bodies),
                expected,
                "accept={accept}: every check is reported once, in order: {bodies:?}"
            );
            let drawn = bodies
                .iter()
                .filter_map(|body| match body {
                    EventBody::Movement {
                        method: wire::MovementMethod::Draw,
                        by_effect,
                        before,
                        after,
                    } => Some((
                        *by_effect,
                        before.player,
                        after.player,
                        after.card.as_ref().map(|card| card.card.printing_id.as_str()),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let expected_draw = if accept {
                vec![(
                    true,
                    Seat::P1,
                    Seat::P1,
                    (viewer == Viewer::P1).then_some("BP01-006"),
                )]
            } else {
                vec![]
            };
            assert_eq!(
                drawn, expected_draw,
                "only an accepted draw trigger draws, as an effect, and only its player \
                 learns which card"
            );
            assert!(
                matches!(bodies.last(), Some(EventBody::Damage {
                    source: Some(Subject::Card { card, object: Some(_) }),
                    target: Some(Subject::Leader { player: Seat::P2 }),
                    class: wire::DamageClass::Attack,
                    amount,
                    life_after: Some(left),
                }) if card.printing_id == "CP03-026" && amount.get() == 5 && left.get() == 15),
                "after the checks the attack deals its damage to the leader: {bodies:?}"
            );
        }
    }
}
