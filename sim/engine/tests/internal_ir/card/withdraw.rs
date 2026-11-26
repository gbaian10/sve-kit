use super::*;
use crate::player_pipeline::{answer, question};
use sve_engine::game::player::events::PlayerEvent;
use sve_engine::game::player::observation::Observation;
use sve_engine::game::player::{
    Command, DecisionShape, Input, Question, QuestionShape, SubmissionKind, Viewer,
};

fn withdraw(game: &mut Game, question: &Question) -> SubmissionKind {
    game.submit_player(
        Viewer::P1,
        Command::Withdraw {
            token: question.token,
        },
    )
    .kind
}

/// Withdraw at `question`, then require the board from before the declaration and a fresh
/// action window.
fn assert_withdrawn(game: &mut Game, asked: &Question, before: &Value, window: &Question) {
    assert!(
        asked.withdrawable,
        "a declaration parameter can be withdrawn"
    );
    assert_eq!(
        withdraw(game, asked),
        SubmissionKind::Reverted,
        "withdrawal returns to the origin"
    );
    assert_eq!(
        &game.projection(View::Referee).unwrap(),
        before,
        "the state is the one before the declaration"
    );
    let again = question(game, Viewer::P1);
    let (QuestionShape::ActionWindow(restored), QuestionShape::ActionWindow(original)) =
        (&again.shape, &window.shape)
    else {
        panic!("action windows");
    };
    assert!(!again.withdrawable, "an action window is not a declaration");
    assert_ne!(
        again.token, window.token,
        "the restored window is a new occurrence"
    );
    assert_eq!(
        restored.intents.len(),
        original.intents.len(),
        "the same intents are offered again"
    );
}

#[test]
fn withdraw_a_card_play_after_its_mode_returns_to_the_action_window() {
    let mut game = play::position(&play::choice_program(), "hand", 1);
    game.advance_player_input();
    let window = question(&game, Viewer::P1);
    let before = game.projection(View::Referee).unwrap();
    let command = player_declarations::play_command(&game, "unit-spell");
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the play is declared"
    );
    assert!(
        question(&game, Viewer::P1).withdrawable,
        "the mode is a declaration parameter"
    );
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Modes(vec![ModeId(1)])).kind,
        SubmissionKind::Applied,
        "the mode is chosen"
    );
    let targets = question(&game, Viewer::P1);
    assert!(
        matches!(
            targets.shape,
            QuestionShape::Decision(DecisionShape::Targets(_))
        ),
        "the chosen mode asks its target"
    );
    assert_withdrawn(&mut game, &targets, &before, &window);
}

#[test]
fn withdraw_an_activation_at_its_cost_choice_returns_to_the_action_window() {
    let mut game = quick_counter::load(
        &quick_counter::initial(true, 2, false),
        &quick_counter::program(true, true),
    );
    game.advance_player_input();
    let window = question(&game, Viewer::P1);
    let before = game.projection(View::Referee).unwrap();
    let command = player_declarations::activation_command(&game, "unit-amulet", 0);
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the activation is declared"
    );
    let cost = question(&game, Viewer::P1);
    assert!(
        matches!(
            cost.shape,
            QuestionShape::Decision(DecisionShape::CostMaterials(_))
        ),
        "the activation asks which counters pay its cost"
    );
    assert_withdrawn(&mut game, &cost, &before, &window);
}

#[test]
fn withdraw_a_quick_activation_returns_to_the_same_quick_window() {
    let mut game = quick_counter::quick_game(
        &quick_counter::initial(true, 2, false),
        &quick_counter::program(true, true),
    );
    game.advance_player_input();
    let window = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Quick { options, .. }) = &window.shape else {
        panic!("a Quick window");
    };
    assert!(!window.withdrawable, "a Quick window is not a declaration");
    let before = game.projection(View::Referee).unwrap();
    let waiting = waiting_view(&game);
    let offered = options.len();
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Quick(Some(options[0].handle))
        )
        .kind,
        SubmissionKind::Applied,
        "the Quick activation is declared"
    );
    let cost = question(&game, Viewer::P1);
    assert!(
        cost.withdrawable,
        "its cost choice is a declaration parameter"
    );
    assert_eq!(withdraw(&mut game, &cost), SubmissionKind::Reverted);
    // The restored window carries a new epoch; everything else is as it was.
    let mut after = game.projection(View::Referee).unwrap();
    after["awaiting"]["epoch"] = before["awaiting"]["epoch"].clone();
    assert_eq!(after, before, "the state is the one before the declaration");
    let again = question(&game, Viewer::P1);
    assert!(
        matches!(&again.shape, QuestionShape::Decision(DecisionShape::Quick { options, .. }) if options.len() == offered),
        "the same Quick window is offered again"
    );
    assert_ne!(again.token, window.token, "it is a new occurrence");
    assert_eq!(waiting_view(&game), waiting, "the other seat saw nothing");
}

#[test]
fn withdraw_is_rejected_outside_a_declaration_and_for_a_retired_question() {
    let mut game = play::position(&play::choice_program(), "hand", 1);
    game.advance_player_input();
    let window = question(&game, Viewer::P1);
    let untouched = exact_authority(&game);
    assert_eq!(
        withdraw(&mut game, &window),
        SubmissionKind::Rejected,
        "an action window has no declaration to withdraw"
    );
    assert_eq!(
        exact_authority(&game),
        untouched,
        "a rejection changes nothing"
    );
    let command = player_declarations::play_command(&game, "unit-spell");
    let _declared = game.submit_player(Viewer::P1, command);
    let modes = question(&game, Viewer::P1);
    assert_eq!(
        game.submit_player(Viewer::P2, Command::Withdraw { token: modes.token })
            .kind,
        SubmissionKind::Rejected,
        "the waiting seat cannot withdraw the actor's declaration"
    );
    assert_eq!(
        withdraw(&mut game, &modes),
        SubmissionKind::Reverted,
        "the actor withdraws"
    );
    let restored = exact_authority(&game);
    assert_eq!(
        withdraw(&mut game, &modes),
        SubmissionKind::Rejected,
        "the withdrawn question is retired"
    );
    assert_eq!(
        exact_authority(&game),
        restored,
        "a rejection changes nothing"
    );
}

/// What the waiting seat can read: its board, its event history and its input.
fn waiting_view(game: &Game) -> (Observation, Vec<PlayerEvent>, Input) {
    (
        game.observe(Viewer::P2).unwrap(),
        game.events(Viewer::P2, None).unwrap().cloned().collect(),
        game.input(Viewer::P2),
    )
}

#[test]
fn the_waiting_seat_learns_nothing_from_a_withdrawn_declaration() {
    // Withdraw at the mode, or at the target after choosing a mode: the waiting seat must
    // see the same thing throughout, and exactly what it saw before the declaration.
    for chosen in [false, true] {
        let mut game = play::position(&play::choice_program(), "hand", 1);
        game.advance_player_input();
        let before = waiting_view(&game);
        let command = player_declarations::play_command(&game, "unit-spell");
        let _declared = game.submit_player(Viewer::P1, command);
        assert_eq!(waiting_view(&game), before, "declaring shows nothing");
        if chosen {
            answer(&mut game, Viewer::P1, Answer::Modes(vec![ModeId(1)]));
            assert_eq!(waiting_view(&game), before, "the chosen mode shows nothing");
        }
        let current = question(&game, Viewer::P1);
        assert_eq!(
            withdraw(&mut game, &current),
            SubmissionKind::Reverted,
            "the actor withdraws"
        );
        assert_eq!(waiting_view(&game), before, "withdrawing leaves no trace");
    }
}
