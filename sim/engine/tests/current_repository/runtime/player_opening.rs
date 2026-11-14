#![expect(
    clippy::panic,
    clippy::too_many_lines,
    reason = "Opening questions and serial protocol assertions fail as one scenario."
)]

use super::*;
use core::array::from_fn;
use sve_engine::game::player::events::{ActionKind, EventKind};
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{
    Command, DeckConstruction, Input, OpeningAnswer, OpeningQuestion, PlayerDeck, Question,
    QuestionShape, StrictStartError, SubmissionKind, Viewer,
};

fn question(game: &Game, viewer: Viewer) -> Question {
    let Input::Question(question) = game.input(viewer) else {
        panic!("opening question");
    };
    *question
}

fn submit(game: &mut Game, actor: Viewer, answer: OpeningAnswer) -> SubmissionKind {
    let question = question(game, actor);
    game.submit_player(
        actor,
        Command::Opening {
            token: question.token,
            answer,
        },
    )
    .kind
}

fn decks() -> [PlayerDeck; 2] {
    // Small runtime fixtures exercise the trusted host boundary, not tournament construction.
    from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: ["BP01-P25", "BP10-122"]
            .into_iter()
            .cycle()
            .take(12)
            .map(str::to_owned)
            .collect(),
        evolve: vec![],
    })
}

#[test]
#[expect(
    clippy::cognitive_complexity,
    reason = "Two identical games verify the same serial protocol and its rejected answers."
)]
fn player_opening_is_serial_private_and_reproducible_from_the_accepted_selection() {
    let create = || {
        Game::start_player_game(selection().clone(), decks(), Viewer::P2, "player-opening").unwrap()
    };
    let mut first = create();
    let mut repeated = create();
    let chooser = question(&first, Viewer::P2);
    assert_eq!(
        chooser.shape,
        QuestionShape::Opening(OpeningQuestion::FirstPlayer)
    );
    assert!(
        matches!(first.input(Viewer::P1), Input::Waiting(wait) if wait.actor == Viewer::P2 && wait.dice.is_none())
    );
    let wrong = first.submit_player(
        Viewer::P1,
        Command::Opening {
            token: chooser.token,
            answer: OpeningAnswer::FirstPlayer(Viewer::P1),
        },
    );
    assert_eq!(wrong.kind, SubmissionKind::Rejected);
    assert_eq!(question(&first, Viewer::P2), chooser);
    for game in [&mut first, &mut repeated] {
        assert_eq!(
            submit(game, Viewer::P2, OpeningAnswer::FirstPlayer(Viewer::P1)),
            SubmissionKind::Applied
        );
    }
    assert_eq!(
        first.observe(Viewer::P1).unwrap(),
        repeated.observe(Viewer::P1).unwrap()
    );
    assert_eq!(
        first.observe(Viewer::P2).unwrap(),
        repeated.observe(Viewer::P2).unwrap()
    );
    let hand = question(&first, Viewer::P1);
    let QuestionShape::Opening(OpeningQuestion::Mulligan(cards)) = hand.shape else {
        panic!("mulligan");
    };
    assert_eq!(cards.len(), 4);
    let other = first.observe(Viewer::P2).unwrap();
    let concealed = other
        .players
        .iter()
        .find(|player| player.player == Viewer::P1)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Hand)
        .unwrap();
    assert_eq!(concealed.count, 4);
    assert_eq!(concealed.cards.len(), 0);
    assert!(!format!("{other:?}").contains(cards[0].handle.as_str()));
    let current = question(&first, Viewer::P1);
    let original = first.projection(View::Referee).unwrap()["P1"]["hand"]
        .as_array()
        .unwrap()
        .clone();
    for answer in [
        OpeningAnswer::FirstPlayer(Viewer::P2),
        OpeningAnswer::Mulligan(vec![]),
        OpeningAnswer::Mulligan(vec![cards[0].handle.clone(); 4]),
    ] {
        assert_eq!(
            submit(&mut first, Viewer::P1, answer),
            SubmissionKind::Rejected
        );
        assert_eq!(question(&first, Viewer::P1), current);
    }
    let mut handles = cards
        .iter()
        .map(|card| card.handle.clone())
        .collect::<Vec<_>>();
    handles.reverse();
    assert_eq!(
        submit(&mut first, Viewer::P1, OpeningAnswer::Mulligan(handles)),
        SubmissionKind::Applied
    );
    let QuestionShape::Opening(OpeningQuestion::Mulligan(same)) =
        question(&repeated, Viewer::P1).shape
    else {
        panic!("mulligan");
    };
    assert_eq!(
        submit(
            &mut repeated,
            Viewer::P1,
            OpeningAnswer::Mulligan(same.iter().rev().map(|card| card.handle.clone()).collect())
        ),
        SubmissionKind::Applied
    );
    assert!(matches!(first.input(Viewer::P1), Input::Waiting(wait) if wait.actor == Viewer::P2));
    assert_eq!(
        first
            .submit_player(
                Viewer::P1,
                Command::Opening {
                    token: current.token,
                    answer: OpeningAnswer::Keep
                }
            )
            .kind,
        SubmissionKind::Rejected
    );
    for game in [&mut first, &mut repeated] {
        assert_eq!(
            submit(game, Viewer::P2, OpeningAnswer::Keep),
            SubmissionKind::Applied
        );
        assert!(matches!(
            question(game, Viewer::P1).shape,
            QuestionShape::ActionWindow(_)
        ));
    }
    assert_eq!(
        first.observe(Viewer::P1).unwrap(),
        repeated.observe(Viewer::P1).unwrap()
    );
    let referee = first.projection(View::Referee).unwrap();
    assert_eq!(referee["P1"]["pp"], json!({"current":1_u8,"max":1_u8}));
    assert_eq!(referee["P1"]["ep"], 0_u8);
    assert_eq!(referee["P2"]["ep"], 3_u8);
    assert_eq!(referee["P1"]["sep"], 1_u8);
    assert_eq!(referee["P2"]["sep"], 1_u8);
    let deck = referee["P1"]["deck"].as_array().unwrap();
    assert_eq!(deck.len(), 8);
    assert_eq!(
        deck.iter().skip(4).cloned().collect::<Vec<_>>(),
        original.into_iter().rev().collect::<Vec<_>>()
    );
}

#[test]
fn player_opening_uses_real_start_amulets_and_bound_initial_resources() {
    let mut players = decks();
    players[0].construction = DeckConstruction::Title("cardfight_vanguard".into());
    players[0].main[0] = "CSD03a-016".into();
    players[1].construction = DeckConstruction::Title("idolmaster_cinderella_girls".into());
    let mut game = Game::start_player_game(
        selection_with_rules().clone(),
        players,
        Viewer::P1,
        "title-opening",
    )
    .unwrap();
    assert_eq!(game.events(Viewer::P2, None).unwrap().count(), 0);
    let original = question(&game, Viewer::P1);
    let QuestionShape::Opening(OpeningQuestion::StartAmulet(cards)) = original.shape else {
        panic!("start amulet");
    };
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].properties.card, "CSD03a-016");
    let resources = game.observe(Viewer::P2).unwrap();
    let ex = resources
        .players
        .iter()
        .find(|player| player.player == Viewer::P2)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Ex)
        .unwrap();
    assert_eq!(ex.count, 5);
    assert_eq!(ex.cards.len(), 5);
    assert_eq!(
        submit(
            &mut game,
            Viewer::P1,
            OpeningAnswer::StartAmulet(cards[0].handle.clone())
        ),
        SubmissionKind::Applied
    );
    let placed = game.events(Viewer::P2, None).unwrap().collect::<Vec<_>>();
    assert_eq!(placed.len(), 1);
    assert!(matches!(&placed[0].kind, EventKind::Movement { after, .. } if after.card.is_none()));
    assert_eq!(
        submit(
            &mut game,
            Viewer::P1,
            OpeningAnswer::FirstPlayer(Viewer::P2)
        ),
        SubmissionKind::Applied
    );
    assert_eq!(
        submit(&mut game, Viewer::P2, OpeningAnswer::Keep),
        SubmissionKind::Applied
    );
    assert_eq!(
        submit(&mut game, Viewer::P1, OpeningAnswer::Keep),
        SubmissionKind::Applied
    );
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::ActionWindow(_)
    ));
    let events = game.events(Viewer::P2, None).unwrap().collect::<Vec<_>>();
    assert_eq!(events.iter().filter(|event| matches!(&event.kind, EventKind::Action(action) if action.kind == ActionKind::Reveal)).count(), 1,
        "The opening amulet is disclosed only after both mulligans");
    assert!(
        matches!(&events.last().unwrap().kind, EventKind::Phase(turn) if turn.phase == Some(Phase::Main))
    );

    for viewer in [Viewer::P1, Viewer::P2] {
        let observation = game.observe(viewer).unwrap();
        let field = observation
            .players
            .iter()
            .find(|player| player.player == Viewer::P1)
            .unwrap()
            .zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Field)
            .unwrap();
        assert_eq!(field.cards.len(), 1);
        assert_eq!(field.cards[0].card.properties.card, "CSD03a-016");
    }
}

#[test]
fn deck_scoped_selection_starts_the_same_game_as_the_whole_selection() {
    let mut decks = decks();
    // BP01-001 transforms cards into a token, so its closure reaches beyond the deck list.
    decks[0].main.push("BP01-001".into());
    let scoped = repository::select_decks(&decks).unwrap();
    for number in ["BP01-P25", "BP10-122", "BP01-001", "BP01-T01"] {
        assert!(scoped.current.contains_key(number), "{number}");
    }
    assert!(scoped.current.len() < 20, "{}", scoped.current.len());
    assert!(
        Game::check_player_decks(&scoped, &decks)
            .unwrap()
            .is_admitted()
    );
    let start =
        |selected| Game::start_player_game(selected, decks.clone(), Viewer::P2, "scoped").unwrap();
    let (scoped, whole) = (start(scoped), start(selection().clone()));
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            format!("{:?}", scoped.input(viewer)),
            format!("{:?}", whole.input(viewer))
        );
        assert_eq!(
            format!("{:?}", scoped.observe(viewer).unwrap()),
            format!("{:?}", whole.observe(viewer).unwrap())
        );
    }
}

#[test]
fn player_opening_load_errors_are_distinct_from_game_submission_errors() {
    let mut absent = decks();
    absent[0].main[0] = "missing-printing".into();
    assert!(matches!(
        Game::start_player_game(selection().clone(), absent, Viewer::P1, "opening").unwrap_err(),
        StrictStartError::Admission(_)
    ));
    let mut short = decks();
    short[0].main.truncate(3);
    assert!(matches!(
        Game::start_player_game(selection().clone(), short, Viewer::P1, "opening").unwrap_err(),
        StrictStartError::Initialization(EngineFailure::Invalid(_))
    ));
    let mut changed = selection().clone();
    changed.current.remove("BP01-P25");
    assert!(matches!(
        Game::start_player_game(changed, decks(), Viewer::P1, "opening").unwrap_err(),
        StrictStartError::Initialization(EngineFailure::Invalid(_))
    ));
    let mut game =
        Game::start_player_game(selection().clone(), decks(), Viewer::P1, "opening").unwrap();
    let original = question(&game, Viewer::P1);
    assert_eq!(
        submit(&mut game, Viewer::P1, OpeningAnswer::Keep),
        SubmissionKind::Rejected
    );
    assert_eq!(question(&game, Viewer::P1), original);
    assert_eq!(
        game.submit_player(Viewer::P2, Command::Concede).kind,
        SubmissionKind::Applied
    );
    assert!(matches!(
        game.input(Viewer::P1),
        Input::Finished {
            winner: Some(Viewer::P1)
        }
    ));
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Opening {
                token: original.token,
                answer: OpeningAnswer::FirstPlayer(Viewer::P1)
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
}
