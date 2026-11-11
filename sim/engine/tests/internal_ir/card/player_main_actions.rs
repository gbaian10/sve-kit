#![expect(clippy::panic, reason = "Unexpected player domains fail the fixture.")]

use super::*;
use crate::player_pipeline::{answer, end_command, question};
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{
    AttackTarget, Command, Input, MainActionAnswer, MainActions, MainIntent, MainIntentKind,
    ObjectHandle, Question, QuestionShape, SubmissionKind, Viewer,
};

fn initial() -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"physical-z","card":"unit-follower"},
        {"id":"physical-a","card":"unit-follower"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy-z","card":"unit-follower","state":{"acted":true}},
        {"id":"enemy-a","card":"unit-follower"}
    ]);
    initial
}

fn board(initial: &Value, abilities: Vec<Located<Ability>>) -> Game {
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "main-intents",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &card(abilities))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    game
}

fn actions(question: &Question) -> &MainActions {
    let QuestionShape::ActionWindow(actions) = &question.shape else {
        panic!("main intents");
    };
    actions
}

fn attack(question: &Question) -> &MainIntent {
    actions(question)
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::Attack { .. }))
        .unwrap()
}

fn command(question: &Question, intent: &MainIntent, target: AttackTarget) -> Command {
    Command::MainAction {
        token: question.token,
        intent: intent.id.clone(),
        answer: MainActionAnswer::Attack(target),
    }
}

fn handle(game: &Game, owner: Viewer, position: u64) -> ObjectHandle {
    game.observe(Viewer::P1)
        .unwrap()
        .players
        .iter()
        .find(|player| player.player == owner)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards
        .iter()
        .find(|card| card.position == Some(position))
        .unwrap()
        .card
        .handle
        .clone()
}

fn keyword(keyword: KeywordId) -> Located<Ability> {
    let mut ability = static_keywords::conditional_keyword();
    let Effect::Continuous {
        condition, patch, ..
    } = &mut ability.node.declarations[0]
    else {
        panic!("keyword grant");
    };
    *condition = None;
    *patch = CharacteristicPatch::GrantAbility {
        abilities: vec![GrantedAbility::Keyword { keyword }],
    };
    ability
}

#[test]
fn player_main_actions_domains_use_public_field_order_and_queries_are_read_only() {
    let game = board(&initial(), vec![]);
    let first = question(&game, Viewer::P1);
    let own = [handle(&game, Viewer::P1, 0), handle(&game, Viewer::P1, 1)];
    let enemy = handle(&game, Viewer::P2, 0);
    assert_eq!(
        actions(&first).intents.len(),
        3,
        "One intent per attacker, followed by EndPhase"
    );
    for (index, intent) in actions(&first).intents.iter().take(2).enumerate() {
        let MainIntentKind::Attack { attacker, targets } = &intent.kind else {
            panic!("attack");
        };
        assert_eq!(attacker.handle, own[index]);
        assert_eq!(
            targets,
            &[
                AttackTarget::Leader(Viewer::P2),
                AttackTarget::Card(enemy.clone())
            ]
        );
        assert!(!format!("{:?}", intent.id).contains("physical"));
    }
    assert!(matches!(
        actions(&first).intents[2].kind,
        MainIntentKind::EndPhase
    ));
    let before = exact_authority(&game);
    for _ in 0..10 {
        assert_eq!(question(&game, Viewer::P1), first);
        assert!(matches!(game.input(Viewer::P2), Input::Waiting(wait) if wait.actor == Viewer::P1));
    }
    assert_eq!(
        exact_authority(&game),
        before,
        "Input preserves RNG, clocks, knowledge and events"
    );
}

#[test]
fn player_main_actions_reject_wrong_seat_target_payload_and_retired_intent() {
    let mut game = board(&initial(), vec![]);
    let first = question(&game, Viewer::P1);
    let selected = attack(&first);
    let valid = command(&first, selected, AttackTarget::Leader(Viewer::P2));
    let before = exact_authority(&game);
    for (actor, bad) in [
        (Viewer::P2, valid.clone()),
        (
            Viewer::P1,
            command(&first, selected, AttackTarget::Leader(Viewer::P1)),
        ),
        (
            Viewer::P1,
            command(
                &first,
                selected,
                AttackTarget::Card(handle(&game, Viewer::P2, 1)),
            ),
        ),
        (
            Viewer::P1,
            Command::MainAction {
                token: first.token,
                intent: selected.id.clone(),
                answer: MainActionAnswer::None,
            },
        ),
    ] {
        assert_eq!(
            game.submit_player(actor, bad).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(question(&game, Viewer::P1), first);
        assert_eq!(exact_authority(&game), before);
    }
    let retired_end = end_command(&first);
    assert_eq!(
        game.submit_player(Viewer::P1, valid.clone()).kind,
        SubmissionKind::Applied
    );
    let quick = question(&game, Viewer::P2);
    assert!(
        matches!(&quick.shape, QuestionShape::Decision(InputDomain::Quick { options, .. }) if options.is_empty())
    );
    assert_eq!(
        answer(&mut game, Viewer::P2, Answer::Quick(None)).kind,
        SubmissionKind::Applied
    );
    let current = question(&game, Viewer::P1);
    assert_ne!(current.token, first.token);
    assert_eq!(
        actions(&current).intents.len(),
        2,
        "The attacker acted; only the other source and EndPhase remain"
    );
    let mut retired_id = retired_end.clone();
    if let Command::MainAction { token, .. } = &mut retired_id {
        *token = current.token;
    }
    let before = exact_authority(&game);
    for bad in [valid, retired_end, retired_id] {
        assert_eq!(
            game.submit_player(Viewer::P1, bad).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(question(&game, Viewer::P1), current);
        assert_eq!(exact_authority(&game), before);
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18
    );
}

#[test]
fn player_main_actions_guard_domain_rejects_other_followers_and_leader() {
    let mut initial = initial();
    initial["players"]["P2"]["zones"]["field"][1]["state"] = json!({"acted":true,"silenced":true});
    let mut game = board(&initial, vec![keyword(KeywordId::Ward)]);
    let first = question(&game, Viewer::P1);
    let selected = attack(&first);
    let MainIntentKind::Attack { targets, .. } = &selected.kind else {
        panic!("attack");
    };
    assert_eq!(targets, &[AttackTarget::Card(handle(&game, Viewer::P2, 0))]);
    let before = exact_authority(&game);
    for target in [
        AttackTarget::Leader(Viewer::P2),
        AttackTarget::Card(handle(&game, Viewer::P2, 1)),
    ] {
        assert_eq!(
            game.submit_player(Viewer::P1, command(&first, selected, target))
                .kind,
            SubmissionKind::Rejected
        );
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn player_main_actions_entry_rush_and_storm_keep_distinct_attack_permissions() {
    for keyword_id in [None, Some(KeywordId::Rush), Some(KeywordId::Storm)] {
        let mut initial = initial();
        for follower in initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
        {
            follower["state"] = json!({"entered_this_turn":true});
        }
        let game = board(&initial, keyword_id.map(keyword).into_iter().collect());
        let first = question(&game, Viewer::P1);
        if keyword_id.is_none() {
            assert_eq!(actions(&first).intents.len(), 1);
            assert!(matches!(
                actions(&first).intents[0].kind,
                MainIntentKind::EndPhase
            ));
        } else {
            let MainIntentKind::Attack { targets, .. } = &attack(&first).kind else {
                panic!("attack");
            };
            assert_eq!(
                targets.contains(&AttackTarget::Leader(Viewer::P2)),
                keyword_id == Some(KeywordId::Storm)
            );
            assert!(targets.contains(&AttackTarget::Card(handle(&game, Viewer::P2, 0))));
        }
    }
}

#[test]
fn player_main_actions_opponent_private_prints_order_and_ids_do_not_change_the_domain() {
    let mut inputs = Vec::new();
    for private in [
        json!([{"id":"hidden-z","card":"unit-spell"},{"id":"hidden-a","card":"unit-amulet"}]),
        json!([{"id":"secret-a","card":"unit-amulet"},{"id":"secret-z","card":"unit-spell"}]),
    ] {
        let mut initial = initial();
        initial["players"]["P2"]["zones"]["hand"] = private.clone();
        initial["players"]["P2"]["zones"]["deck"] = private
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, card)| json!({"id":format!("deck-{index}"),"card":card["card"]}))
            .collect();
        let game = board(&initial, vec![]);
        inputs.push(game.input(Viewer::P1));
    }
    assert_eq!(
        inputs[0], inputs[1],
        "Every public intent field and opaque option is unchanged"
    );
}

#[test]
fn player_main_actions_attack_pending_finishes_before_the_empty_quick_window_and_main() {
    let trigger = timing::on_attack(ability(1, vec![draw(1, 0)]));
    let mut game = board(&initial(), vec![trigger]);
    let first = question(&game, Viewer::P1);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            command(&first, attack(&first), AttackTarget::Leader(Viewer::P2))
        )
        .kind,
        SubmissionKind::Applied
    );
    let pending = question(&game, Viewer::P1);
    let QuestionShape::Decision(InputDomain::Pending(options)) = pending.shape else {
        panic!("attack pending");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Pending(options[0].handle)).kind,
        SubmissionKind::Applied
    );
    assert!(
        matches!(&question(&game, Viewer::P2).shape, QuestionShape::Decision(InputDomain::Quick { options, .. }) if options.is_empty())
    );
    assert_eq!(
        answer(&mut game, Viewer::P2, Answer::Quick(None)).kind,
        SubmissionKind::Applied
    );
    assert_eq!(actions(&question(&game, Viewer::P1)).intents.len(), 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18
    );
}

#[test]
fn player_main_actions_unavailable_authority_is_a_persistent_fault_for_both_viewers() {
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "main-fault",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    let before = exact_authority(&game);
    for _ in 0..2 {
        assert_eq!(game.input(Viewer::P1), Input::EngineFault);
        assert_eq!(game.input(Viewer::P2), Input::EngineFault);
        game.advance_player_input();
        assert_eq!(exact_authority(&game), before);
    }
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
}
