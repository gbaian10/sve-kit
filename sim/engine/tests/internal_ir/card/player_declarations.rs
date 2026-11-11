#![expect(
    clippy::panic,
    reason = "Unexpected player declaration domains fail the fixture."
)]

use super::*;
use crate::player_pipeline::question;
use sve_engine::game::player::events::{ActionKind, EventKind};
use sve_engine::game::player::{
    AttackTarget, Command, Input, MainActionAnswer, MainIntentKind, Question, QuestionShape,
    SubmissionKind, Viewer,
};

pub(super) fn play_command(game: &Game, number: &str) -> Command {
    let question = question(game, Viewer::P1);
    let QuestionShape::ActionWindow(actions) = question.shape else {
        panic!("main declarations");
    };
    let selected = actions.intents.iter().find(|intent| matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == number)).unwrap();
    Command::MainAction {
        token: question.token,
        intent: selected.id.clone(),
        answer: MainActionAnswer::None,
    }
}

pub(super) fn activation_command(game: &Game, number: &str, ordinal: usize) -> Command {
    let question = question(game, Viewer::P1);
    let QuestionShape::ActionWindow(actions) = question.shape else {
        panic!("main activations");
    };
    let selected = actions.intents.iter().filter(|intent| matches!(&intent.kind, MainIntentKind::Activate { holder, .. } if holder.properties.card == number)).nth(ordinal).unwrap();
    Command::MainAction {
        token: question.token,
        intent: selected.id.clone(),
        answer: MainActionAnswer::None,
    }
}

fn board(
    initial: &Value,
    spell: &WellFormedCardProgram,
    follower: &WellFormedCardProgram,
    seed: &str,
) -> Game {
    let mut game = Game::new(catalog(), initial, &Value::Null, &Value::Null, seed).unwrap();
    game.load_internal_card("unit-spell", spell).unwrap();
    game.load_internal_card("unit-follower", follower).unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    game
}

fn has_play(question: &Question) -> bool {
    let QuestionShape::ActionWindow(actions) = &question.shape else {
        panic!("main actions");
    };
    actions
        .intents
        .iter()
        .any(|intent| matches!(intent.kind, MainIntentKind::Play { .. }))
}

#[test]
fn player_declarations_hand_and_ex_spell_cost_is_checked_and_paid_once() {
    let program = card(vec![play::spell(vec![draw(1, 1)])]);
    for zone in ["hand", "ex"] {
        for pp in [0, 1] {
            let mut initial = setup(1, json!([{"id":"d","card":"unit-follower"}]));
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"s","card":"unit-spell"}]);
            initial["players"]["P1"]["pp"] = json!({"current":pp,"max":1});
            let mut game = board(&initial, &program, &card(vec![]), "main-play");
            let first = question(&game, Viewer::P1);
            assert_eq!(has_play(&first), pp == 1);
            let before = exact_authority(&game);
            for _ in 0..3 {
                assert_eq!(question(&game, Viewer::P1), first);
            }
            assert_eq!(exact_authority(&game), before);
            if pp == 1 {
                let command = play_command(&game, "unit-spell");
                assert_eq!(
                    game.submit_player(Viewer::P1, command).kind,
                    SubmissionKind::Applied
                );
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["P1"]["pp"]["current"], 0);
                assert_eq!(state["P1"]["hand"], json!(["d"]));
                assert_eq!(state["P1"]["cemetery"], json!(["s"]));
                assert!(matches!(game.input(Viewer::P1), Input::Question(_)));
            }
        }
    }
}

#[test]
fn player_declarations_wrong_actor_payload_and_stale_play_leave_the_question_and_rng() {
    let program = card(vec![play::spell(vec![draw(1, 1)])]);
    let initial = setup(1, json!([{"id":"d","card":"unit-follower"}]));
    let mut game = board(&initial, &program, &card(vec![]), "main-protocol");
    let command = play_command(&game, "unit-spell");
    let mut wrong_payload = command.clone();
    if let Command::MainAction { answer, .. } = &mut wrong_payload {
        *answer = MainActionAnswer::Attack(AttackTarget::Leader(Viewer::P2));
    }
    let first = question(&game, Viewer::P1);
    let before = exact_authority(&game);
    for (actor, invalid) in [(Viewer::P2, command.clone()), (Viewer::P1, wrong_payload)] {
        assert_eq!(
            game.submit_player(actor, invalid).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(question(&game, Viewer::P1), first);
        assert_eq!(exact_authority(&game), before);
    }
    assert_eq!(
        game.submit_player(Viewer::P1, command.clone()).kind,
        SubmissionKind::Applied
    );
    let before = exact_authority(&game);
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn player_declarations_source_posture_and_pp_are_prechecked_without_paying() {
    let program = card(vec![payment::paid_ability(
        vec![payment::pp(1), payment::act()],
        vec![draw(1, 0)],
    )]);
    for (acted, pp) in [(false, 1), (true, 1), (false, 0)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower","state":{"acted":acted}}]);
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":1});
        let mut game = board(&initial, &card(vec![]), &program, "main-activation");
        let first = question(&game, Viewer::P1);
        let QuestionShape::ActionWindow(actions) = &first.shape else {
            panic!("main actions");
        };
        assert_eq!(
            actions
                .intents
                .iter()
                .filter(|intent| matches!(intent.kind, MainIntentKind::Activate { .. }))
                .count(),
            usize::from(!acted && pp == 1)
        );
        let before = exact_authority(&game);
        assert_eq!(question(&game, Viewer::P1), first);
        assert_eq!(exact_authority(&game), before);
        if !acted && pp == 1 {
            let command = activation_command(&game, "unit-follower", 0);
            assert_eq!(
                game.submit_player(Viewer::P1, command).kind,
                SubmissionKind::Applied
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 0);
            assert_eq!(state["objects"]["source"]["acted"], true);
        }
    }
}

#[test]
fn player_declarations_full_field_blocks_plain_follower_but_not_spell() {
    let spell = card(vec![play::spell(vec![draw(1, 0)])]);
    for number in ["unit-spell", "unit-follower"] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..5)
                .map(|index| json!({"id":format!("own-{index}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let game = board(&initial, &spell, &card(vec![]), "main-capacity");
        assert_eq!(
            has_play(&question(&game, Viewer::P1)),
            number == "unit-spell"
        );
    }
}

#[test]
fn player_declarations_full_field_exchange_frees_a_slot_before_entry() {
    let tree = exchange::candidate(
        "unit-follower",
        json!({"optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"follower"},"n":1}},"optional_cost_delta":0}),
        false,
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for accept in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..5)
                .map(|index| json!({"id":format!("own-{index}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut game = board(&initial, &card(vec![]), &program, "main-capacity-exchange");
        let before = exact_authority(&game);
        assert!(has_play(&question(&game, Viewer::P1)));
        assert_eq!(exact_authority(&game), before);
        let command = play_command(&game, "unit-follower");
        assert_eq!(
            game.submit_player(Viewer::P1, command).kind,
            SubmissionKind::Applied
        );
        let result = player_pipeline::answer(&mut game, Viewer::P1, Answer::OptionalCost(accept));
        if accept {
            assert_eq!(result.kind, SubmissionKind::Applied);
            let paid =
                player_pipeline::answer(&mut game, Viewer::P1, Answer::CostMaterials(vec![0]));
            assert_eq!(paid.kind, SubmissionKind::Applied);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["field"],
                json!(["own-1", "own-2", "own-3", "own-4", "s"])
            );
            assert_eq!(state["P1"]["cemetery"], json!(["own-0"]));
            assert_eq!(state["P1"]["pp"]["current"], 1);
        } else {
            assert_eq!(result.kind, SubmissionKind::Reverted);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["s"])
            );
        }
    }
}

#[test]
fn player_declarations_opponent_hidden_cards_do_not_change_play_or_activation_options() {
    let spell = card(vec![play::spell(vec![draw(1, 1)])]);
    let follower = card(vec![ability(1, vec![draw(1, 0)])]);
    let mut inputs = Vec::new();
    for hidden in [
        json!([{"id":"private-z","card":"unit-spell"},{"id":"private-a","card":"unit-amulet"}]),
        json!([{"id":"hidden-a","card":"unit-amulet"},{"id":"hidden-z","card":"unit-spell"}]),
    ] {
        let mut initial = setup(1, json!([{"id":"d","card":"unit-follower"}]));
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"caster","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["hand"] = hidden.clone();
        initial["players"]["P2"]["zones"]["deck"] = hidden
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, card)| json!({"id":format!("deck-{index}"),"card":card["card"]}))
            .collect();
        let game = board(&initial, &spell, &follower, "main-hidden-pair");
        let first = question(&game, Viewer::P1);
        let QuestionShape::ActionWindow(actions) = &first.shape else {
            panic!("main actions");
        };
        assert!(
            actions
                .intents
                .iter()
                .any(|intent| matches!(intent.kind, MainIntentKind::Play { .. }))
        );
        assert!(
            actions
                .intents
                .iter()
                .any(|intent| matches!(intent.kind, MainIntentKind::Activate { .. }))
        );
        inputs.push(first);
    }
    assert_eq!(inputs[0], inputs[1]);
}

#[test]
fn player_declarations_rng_does_not_choose_intents_or_run_the_spell_body() {
    let program = card(vec![play::spell(
        (1..=10)
            .map(|id| {
                instruction(
                    id,
                    Action::Dice {
                        operation: DiceOperation::Roll {},
                        result: Binding::new(20 + id, ScopeId(1)),
                    },
                )
            })
            .collect(),
    )]);
    let initial = setup(1, json!([]));
    let mut views = Vec::new();
    let mut results = Vec::new();
    for seed in ["declaration-private-rng-a", "declaration-private-rng-b"] {
        let mut game = board(&initial, &program, &card(vec![]), seed);
        let first = question(&game, Viewer::P1);
        let before = exact_authority(&game);
        for _ in 0..10 {
            assert_eq!(question(&game, Viewer::P1), first);
        }
        assert_eq!(exact_authority(&game), before);
        assert_eq!(game.events(Viewer::P1, None).unwrap().count(), 0);
        let command = play_command(&game, "unit-spell");
        assert_eq!(
            game.submit_player(Viewer::P1, command).kind,
            SubmissionKind::Applied
        );
        let dice = game
            .events(Viewer::P1, None)
            .unwrap()
            .filter_map(|event| match &event.kind {
                EventKind::Action(action) if action.kind == ActionKind::Dice => {
                    Some(action.numbers.clone())
                }
                EventKind::Movement { .. }
                | EventKind::Action(_)
                | EventKind::Phase(_)
                | EventKind::Result { .. } => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(dice.len(), 10);
        views.push(first);
        results.push(dice);
    }
    let first = views.remove(0);
    let mut second = views.remove(0);
    let (QuestionShape::ActionWindow(left), QuestionShape::ActionWindow(right)) =
        (&first.shape, &mut second.shape)
    else {
        panic!("main actions");
    };
    assert_eq!(left.intents.len(), right.intents.len());
    for (left, right) in left.intents.iter().zip(&mut right.intents) {
        right.id = left.id.clone();
        match (&left.kind, &mut right.kind) {
            (MainIntentKind::Play { card: left }, MainIntentKind::Play { card: right }) => {
                right.handle = left.handle.clone();
            }
            (MainIntentKind::EndPhase, MainIntentKind::EndPhase) => {}
            _ => panic!("private seed changed the visible declarations"),
        }
    }
    assert_eq!(
        first, second,
        "Fresh opaque streams may be renamed; every visible field and order agrees"
    );
    assert_ne!(
        results[0], results[1],
        "The body consumes genuinely different private RNG streams only after declaration"
    );
}
