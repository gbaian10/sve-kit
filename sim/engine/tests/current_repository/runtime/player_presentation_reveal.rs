
use super::player_presentation::{SEATS, answer, declare, is_action, kinds, present, question};
use super::*;
use sve_engine::game::player::events::{ActionKind, PlayerEvent, PresentationCursor};
use sve_engine::game::player::{DecisionShape, QuestionShape, SubmissionKind};

const DISTINCT: [&str; 9] = [
    "BP19-P31", "BP01-176", "BP01-170", "BP01-171", "BP01-174", "BP01-177", "BP01-179", "BP19-115",
    "BP19-119",
];

/// BP19-113's Fanfare reveals the top nine cards, shuffles them back and then asks a private
/// Advance question: at that stop nobody knows where the revealed cards are any more.
fn zerael() -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-113"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"advance","card":"BP19-P32"}]);
    let mut deck = DISTINCT
        .iter()
        .enumerate()
        .map(|(index, number)| json!({"id":format!("d{index}"),"card":number}))
        .collect::<Vec<_>>();
    let shuffled = (0..deck.len())
        .rev()
        .map(|index| format!("d{index}"))
        .collect::<Vec<_>>();
    deck.push(json!({"id":"tail","card":"BP01-176"}));
    initial["players"]["P1"]["zones"]["deck"] = json!(deck);
    let random = json!({"shuffles":[{"player":"P1","result":shuffled}]});
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &random,
        "zerael-presentation",
    )
    .unwrap();
    game.advance_player_input();
    game
}

fn reveals(events: &[PlayerEvent]) -> usize {
    events
        .iter()
        .filter(|event| is_action(&event.kind, ActionKind::Reveal))
        .count()
}

/// Play the follower and start its Fanfare, handing both seats a presentation at every stop
/// on the way, up to the private Advance question. Returns each seat's committed history
/// length before the Fanfare.
fn advance_question(game: &mut Game, cursors: &mut [Option<PresentationCursor>; 2]) -> [usize; 2] {
    assert_eq!(
        present(game, cursors),
        [vec![], vec![]],
        "the baseline handoff shows nothing; it only records what each seat can identify"
    );
    declare(game, "BP19-113");
    let QuestionShape::Decision(DecisionShape::Pending(options)) = question(game).shape else {
        panic!("pending Fanfare");
    };
    assert_eq!(
        present(game, cursors),
        [vec![], vec![]],
        "the card play already committed; the pending choice suspends no execution"
    );
    let entered = SEATS.map(|viewer| history(game, viewer).len());
    assert_eq!(
        answer(game, Answer::Pending(options[0].handle)),
        SubmissionKind::Applied,
        "the Fanfare is played, reveals nine cards and shuffles them back"
    );
    assert!(
        matches!(
            question(game).shape,
            QuestionShape::Decision(DecisionShape::Targets(_))
        ),
        "the private Advance choice is a real stop after the shuffle"
    );
    entered
}

#[test]
fn cards_revealed_and_shuffled_back_before_the_stop_are_not_shown_early() {
    let mut game = zerael();
    let mut cursors = [None, None];
    let entered = advance_question(&mut game, &mut cursors);
    let shown = present(&mut game, &mut cursors);
    for ((viewer, batches), before) in SEATS.into_iter().zip(&shown).zip(entered) {
        let kinds = kinds(batches);
        assert!(
            !kinds.iter().any(|kind| is_action(kind, ActionKind::Reveal)),
            "no board this seat was handed contains the revealed cards any more, so their \
             reveal is not shown before it commits: {kinds:?}"
        );
        assert!(
            kinds
                .iter()
                .any(|kind| is_action(kind, ActionKind::AbilityPlay)),
            "the played Fanfare itself, which precedes the reveal, is still shown: {kinds:?}"
        );
        assert_eq!(
            history(&game, viewer).len(),
            before,
            "nothing of the Fanfare is committed yet"
        );
    }

    assert_eq!(
        answer(&mut game, Answer::Targets(vec![])),
        SubmissionKind::Applied,
        "declining the Advance completes the Fanfare"
    );
    for (viewer, batches) in SEATS.into_iter().zip(&shown) {
        let events = history(&game, viewer);
        assert_eq!(
            reveals(&events),
            DISTINCT.len(),
            "every public reveal still reaches committed history with its original permissions"
        );
        assert!(
            events
                .iter()
                .filter(|event| is_action(&event.kind, ActionKind::Reveal))
                .all(|event| event.presentation.is_none()),
            "a reveal that was never shown early is ordinary history"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.presentation.is_some())
                .count(),
            batches
                .iter()
                .map(|batch| batch.events.len())
                .sum::<usize>(),
            "exactly what was shown early is promoted"
        );
    }
}
