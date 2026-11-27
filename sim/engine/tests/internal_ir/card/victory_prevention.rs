use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::{Input, SubmissionKind, Viewer, events::EventKind};

fn tree() -> Value {
    let mut input = evolution_linked_movement::tree(&[
        (
            "static",
            json!({"effects":[{"cannot":{"what":"lose","who":"self"}},{"cannot":{"what":"win","who":"opp"}}]}),
        ),
        (
            "auto",
            json!({"on":{"life_changed":{"who":"self","crosses_zero":true}},"do":[{"move":{"ref":"this","to":"cemetery"}},{"set_life":{"who":"self","n":1}}]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":-21}},{"life":{"who":"self","delta":-2}}]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"win":{"who":"self"}}]}),
        ),
        ("activate", json!({"cost":{"pp":0},"do":[{"draw":{"n":1}}]})),
    ]);
    let producer = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":-2}}]}),
    )]);
    input["cards"]["unit-amulet"] = producer["cards"]["unit-follower"].clone();
    input
}
fn loaded(count: usize) -> (Game, WellFormedCardProgram, AbilityId) {
    let programs = lowerer().lower(&tree()).unwrap();
    let program = programs["unit-follower"].clone();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..count)
            .map(|n| json!({"id":format!("guard-{n}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"unit-follower","state":{"silenced":true}}]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"unit-amulet"}));
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    let producer = &programs["unit-amulet"];
    game.load_internal_card("unit-amulet", producer).unwrap();
    (game, program, producer.card().abilities[0].node.id)
}
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn activate(game: &mut Game, program: &WellFormedCardProgram, index: usize) -> CardPlay {
    let result = game
        .activate_internal_card(
            "guard-0",
            program.card().abilities[index].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap();
    finish(game);
    result
}
#[test]
fn victory_prevention_crosses_zero_once_and_resolves_every_guardian_after_both_changes() {
    for count in [1, 2] {
        let (mut game, program, producer) = loaded(count);
        resolved(activate(&mut game, &program, 2));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            -3
        );
        let mut remaining = count;
        while let Some(question) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("pending life occurrence")
            };
            assert_eq!(
                options.len(),
                remaining,
                "only the positive-to-nonpositive crossing triggers"
            );
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            finish(&mut game);
            remaining -= 1;
        }
        assert_eq!(remaining, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 1);
        assert_eq!(state["P1"]["field"], json!(["producer"]));
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), count);
        assert_ne!(state["game"]["ended"], true);
        game.activate_internal_card("producer", producer, PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["game"]["winner"],
            "P2"
        );
    }
}
#[test]
fn victory_prevention_blocks_empty_deck_loss_but_allows_concession_and_own_effect_win() {
    let (mut game, program, _) = loaded(1);
    resolved(activate(&mut game, &program, 4));
    assert_ne!(
        game.projection(View::Referee).unwrap()["game"]["ended"],
        true
    );
    game.concede_internal(PlayerId { seat: 1 }).unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["game"]["winner"],
        "P2"
    );
    let (mut game, program, _) = loaded(1);
    assert!(matches!(
        activate(&mut game, &program, 3),
        CardPlay::GameEnded(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["game"]["winner"],
        "P1"
    );
}
#[test]
fn victory_prevention_is_removed_by_silence_and_the_opponents_effect_victory_resolves_without_winning()
 {
    let program = lowerer().lower(&tree()).unwrap()["unit-follower"].clone();
    // The opponent's guardian forbids P1's victory; its own printed aura remains active.
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"guard-0","card":"unit-follower"}]);
    let mut blocked = evolution_linked_movement::loaded(&initial, &program);
    resolved(activate(&mut blocked, &program, 3));
    assert_ne!(
        blocked.projection(View::Referee).unwrap()["game"]["ended"],
        true
    );
    let mut silenced = setup(1, json!([]));
    silenced["players"]["P1"]["zones"]["hand"] = json!([]);
    silenced["players"]["P1"]["leader"]["life"] = json!(0);
    silenced["players"]["P1"]["zones"]["field"] =
        json!([{"id":"guard-0","card":"unit-follower","state":{"silenced":true}}]);
    let mut silenced = evolution_linked_movement::loaded(&silenced, &program);
    silenced
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    assert_eq!(
        silenced.projection(View::Referee).unwrap()["game"]["winner"],
        "P2"
    );
}
#[test]
fn victory_prevention_rejects_conditional_nonfield_and_installed_player_permissions() {
    for patch in [json!({"while":{"awakened":true}}), json!({"zone":"ex"})] {
        let mut input = tree();
        for (key, value) in patch.as_object().unwrap() {
            input["cards"]["unit-follower"]["abilities"][0]["static"][key] = value.clone();
        }
        lowerer().lower(&input).unwrap_err();
    }
    let input = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"apply":{"on":"this","until":"turn","effect":{"cannot":{"what":"lose","who":"self"}}}}]}),
    )]);
    lowerer().lower(&input).unwrap_err();
}

#[test]
fn direct_opponent_loss_rejects_self_and_all_players() {
    for who in ["self", "all"] {
        let input = evolution_linked_movement::tree(&[(
            "activate",
            json!({"cost":{"pp":0},"do":[{"lose":{"who":who}}]}),
        )]);
        lowerer().lower(&input).unwrap_err();
    }
}

#[test]
fn direct_opponent_loss_ends_resolution_unless_that_player_cannot_lose() {
    for (who, restriction) in [
        ("opp", None),
        ("opponents", Some(("win", "opp"))),
        ("opponents", Some(("lose", "self"))),
    ] {
        let mut input = evolution_linked_movement::tree(&[(
            "activate",
            json!({"cost":{"pp":0},"do":[
                {"lose":{"who":who}},
                {"life":{"who":"self","delta":1}}
            ]}),
        )]);
        if let Some((what, protected)) = restriction {
            let guardian = evolution_linked_movement::tree(&[(
                "static",
                json!({"effects":[{"cannot":{"what":what,"who":protected}}]}),
            )]);
            input["cards"]["unit-amulet"] = guardian["cards"]["unit-follower"].clone();
        }
        let programs = lowerer().lower(&input).unwrap();
        let defeat = &programs["unit-follower"];
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = if restriction.is_some() {
            json!([{"id":"guardian","card":"unit-amulet"}])
        } else {
            json!([])
        };
        let mut game = evolution_linked_movement::loaded(&initial, defeat);
        if let Some(guardian) = programs.get("unit-amulet") {
            game.load_internal_card("unit-amulet", guardian).unwrap();
        }
        game.advance_player_input();
        let command = player_declarations::activation_command(&game, "unit-follower", 0);
        assert_eq!(
            game.submit_player(Viewer::P1, command).kind,
            SubmissionKind::Applied
        );
        let blocked = restriction == Some(("lose", "self"));
        assert_eq!(
            matches!(
                game.input(Viewer::P1),
                Input::Finished {
                    winner: Some(Viewer::P1)
                }
            ),
            !blocked
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 20_u8 + u8::from(blocked));
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        if !blocked {
            assert_eq!(state["game"]["winner"], "P1");
            assert_eq!(state["game"]["by"], "1.2.4");
        }
        let results = game
            .events(Viewer::P1, None)
            .unwrap()
            .filter_map(|event| {
                if let EventKind::Result { winner, loser, .. } = event.kind {
                    Some((winner, loser))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let expected = if blocked {
            vec![]
        } else {
            vec![(None, Some(Viewer::P2))]
        };
        assert_eq!(results, expected);
    }
}
