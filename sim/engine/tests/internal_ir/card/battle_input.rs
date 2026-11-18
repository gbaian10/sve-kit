#![expect(
    clippy::panic,
    reason = "Unexpected public input domains fail the fixture."
)]

use super::*;
use crate::{board_fixture, board_fixtures};
use sve_battle_contract::{
    Answer as WireAnswer, BattleInput, Command as WireCommand, QuestionId, Seat, question as wire,
};
use sve_engine::battle::{AnswerError, BattleReferences, InputProjection};
use sve_engine::game::player::{SubmissionKind, Viewer};

#[path = "battle_input/lifecycle.rs"]
mod lifecycle;

fn opposite(actor: Viewer) -> Viewer {
    if actor == Viewer::P1 {
        Viewer::P2
    } else {
        Viewer::P1
    }
}
fn seat(actor: Viewer) -> Seat {
    if actor == Viewer::P1 {
        Seat::P1
    } else {
        Seat::P2
    }
}
fn project(game: &Game, viewer: Viewer) -> InputProjection {
    project_with(game, viewer, &board_fixture::fixture().unwrap().references)
}
fn project_with(game: &Game, viewer: Viewer, references: &BattleReferences) -> InputProjection {
    game.battle_input(viewer, &QuestionId::new("host-stop".into()), references)
        .unwrap()
}
fn question(input: &InputProjection) -> &wire::Question {
    let BattleInput::Question { question } = input.input() else {
        panic!("wire question")
    };
    question
}
fn intents(input: &InputProjection) -> &[wire::MainIntent] {
    let wire::QuestionShape::ActionWindow { intents } = &question(input).shape else {
        panic!("main actions")
    };
    intents
}
fn submit(game: &mut Game, actor: Viewer, input: &InputProjection, answer: WireAnswer) {
    let command = input
        .command(
            actor,
            &WireCommand::Answer {
                question_id: question(input).id.clone(),
                answer,
            },
        )
        .unwrap();
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "a projected answer must advance the native authority"
    );
}
fn board(initial: &Value, follower: &WellFormedCardProgram, spell: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        initial,
        &Value::Null,
        &Value::Null,
        "complete-input",
    )
    .unwrap();
    game.load_internal_card("unit-follower", follower).unwrap();
    game.load_internal_card("unit-spell", spell).unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn wire_input_play_and_activation_use_bound_sources_and_commit_the_native_costs() {
    let mut initial = board_fixtures::setup(1, json!([{"id":"drawn","card":"unit-amulet"}]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"holder","card":"unit-follower"}]);
    let mut game = board(
        &initial,
        &card(vec![payment::paid_ability(
            vec![payment::pp(1), payment::act()],
            vec![draw(1, 0)],
        )]),
        &card(vec![play::spell(vec![draw(1, 1)])]),
    );
    let before = exact_authority(&game);
    let input = project(&game, Viewer::P1);
    assert_eq!(
        input.observation(),
        &game
            .battle_observation(Viewer::P1, &board_fixture::fixture().unwrap().references)
            .unwrap()
    );
    assert!(question(&input).is_coherent());
    let play = intents(&input)
        .iter()
        .find_map(|intent| {
            if let wire::MainIntent::Play { id, source, cost } = intent {
                assert_eq!(
                    input.observation().objects[source].card.printing_id,
                    "printing-spell"
                );
                assert!(cost.is_some());
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        input.answer(
            Viewer::P2,
            &question(&input).id,
            &WireAnswer::ActionWindow {
                intent: play.clone(),
                target: None
            }
        ),
        Err(AnswerError)
    );
    assert_eq!(exact_authority(&game), before);
    submit(
        &mut game,
        Viewer::P1,
        &input,
        WireAnswer::ActionWindow {
            intent: play,
            target: None,
        },
    );
    let activation = project(&game, Viewer::P1);
    let activate = intents(&activation)
        .iter()
        .find_map(|intent| {
            if let wire::MainIntent::Activate {
                id,
                holder,
                ability,
            } = intent
            {
                assert_eq!(
                    activation.observation().objects[holder].card.printing_id,
                    "printing-follower"
                );
                assert!(matches!(ability,
                    sve_battle_contract::SourceRef::Card {
                        printing_id, holder: Some(source), ..
                    } if printing_id == "printing-follower" && source == holder
                ));
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap();
    submit(
        &mut game,
        Viewer::P1,
        &activation,
        WireAnswer::ActionWindow {
            intent: activate,
            target: None,
        },
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0);
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["cemetery"], json!(["s"]));
    assert_eq!(state["objects"]["holder"]["acted"], true);
}

#[test]
fn wire_input_attack_targets_belong_to_their_own_intent_and_end_phase_advances() {
    let mut initial = board_fixtures::setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"attacker-a","card":"unit-follower"},{"id":"attacker-b","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"unit-follower","state":{"acted":true}}]);
    let game = board(&initial, &card(vec![]), &card(vec![]));
    let input = project(&game, Viewer::P1);
    let attacks = intents(&input)
        .iter()
        .filter_map(|intent| {
            if let wire::MainIntent::Attack {
                id,
                attacker,
                targets,
            } = intent
            {
                Some((id, attacker, targets))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(attacks.len(), 2);
    let (id, _, targets) = attacks[0];
    assert_eq!(
        targets
            .iter()
            .map(|option| &option.target)
            .collect::<Vec<_>>(),
        attacks[1]
            .2
            .iter()
            .map(|option| &option.target)
            .collect::<Vec<_>>()
    );
    assert_eq!(targets.len(), 2);
    let before = exact_authority(&game);
    assert_eq!(
        input.answer(
            Viewer::P1,
            &question(&input).id,
            &WireAnswer::ActionWindow {
                intent: id.clone(),
                target: Some(attacks[1].2[0].id.clone())
            }
        ),
        Err(AnswerError)
    );
    assert_eq!(exact_authority(&game), before);
    let mut attack_game = game.clone();
    submit(
        &mut attack_game,
        Viewer::P1,
        &input,
        WireAnswer::ActionWindow {
            intent: id.clone(),
            target: Some(targets[0].id.clone()),
        },
    );
    let end = intents(&input)
        .iter()
        .find_map(|intent| {
            if let wire::MainIntent::EndPhase { id } = intent {
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap();
    let mut end_game = game;
    submit(
        &mut end_game,
        Viewer::P1,
        &input,
        WireAnswer::ActionWindow {
            intent: end,
            target: None,
        },
    );
    assert_eq!(
        question(&project(&end_game, Viewer::P2)).usage,
        wire::QuestionUse::Quick
    );
}

#[test]
fn wire_input_evolve_begins_the_existing_form_and_payment_flow() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let (base, form) = evolution::programs();
    let mut game = evolution::position_with_catalog(catalog, 1, base, form, false, &Value::Null);
    game.advance_player_input();
    let input = project_with(&game, Viewer::P1, &references);
    let evolve = intents(&input)
        .iter()
        .find_map(|intent| {
            if let wire::MainIntent::Evolve { id, ability, .. } = intent {
                assert!(matches!(ability,
                    sve_battle_contract::SourceRef::Card { printing_id, .. }
                        if printing_id == "printing-base"
                ));
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap();
    submit(
        &mut game,
        Viewer::P1,
        &input,
        WireAnswer::ActionWindow {
            intent: evolve,
            target: None,
        },
    );
    let form = project_with(&game, Viewer::P1, &references);
    let wire::QuestionShape::Choose { options, .. } = &question(&form).shape else {
        panic!("form choice")
    };
    submit(
        &mut game,
        Viewer::P1,
        &form,
        WireAnswer::Choose {
            chosen: vec![options[0].id.clone()],
        },
    );
    let payment = project_with(&game, Viewer::P1, &references);
    let wire::QuestionShape::EvolutionPayment { payments } = &question(&payment).shape else {
        panic!("payment")
    };
    let choice = payments[0];
    submit(
        &mut game,
        Viewer::P1,
        &payment,
        WireAnswer::EvolutionPayment {
            pp: choice.pp,
            use_ep: choice.use_ep,
            super_evolution: choice.super_evolution,
        },
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        1
    );
}

#[test]
fn wire_input_main_and_waiting_ignore_the_other_seats_private_action_domain() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut own_views = Vec::new();
        let mut waiting_views = Vec::new();
        for secret in ["unit-spell", "unit-amulet"] {
            let mut game = board_fixture::board(actor, secret, &card(vec![])).unwrap();
            game.advance_player_input();
            own_views.push(project(&game, actor));
            let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
            let (own, other) = if actor == Viewer::P1 {
                ("P1", "P2")
            } else {
                ("P2", "P1")
            };
            initial["turn"]["active"] = json!(own);
            initial["players"][own]["zones"]["hand"] = json!([{"id":"private","card":secret}]);
            initial["players"][other]["zones"]["hand"] = json!([]);
            let game = board(
                &initial,
                &card(vec![]),
                &card(vec![play::spell(vec![draw(1, 0)])]),
            );
            let waiting = project(&game, opposite(actor));
            assert!(
                matches!(waiting.input(),BattleInput::Waiting {actor:by,context} if *by==seat(actor) && context.dice.is_none())
            );
            assert_eq!(
                waiting.command(actor, &WireCommand::Concede),
                Err(AnswerError)
            );
            waiting_views.push(waiting);
        }
        for views in [own_views, waiting_views] {
            assert_eq!(views[0].observation(), views[1].observation());
            assert_eq!(views[0].input(), views[1].input());
        }
    }
}
