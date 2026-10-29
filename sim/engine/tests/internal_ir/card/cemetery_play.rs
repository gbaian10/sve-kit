#![expect(
    clippy::panic,
    reason = "Unexpected authority questions invalidate these synthetic scenarios."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

fn tree() -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["line"] = json!(1);
    entry["auto"] = json!({
        "zone":"cemetery", "on":{"played":{"who":"self","sel":{"kind":"spell","class":"ニュートラル"},"during":{"turn":"self"}}},
        "cost":{"pp":1}, "do":[{"move":{"ref":"this","to":"field"}}]
    });
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn initial(pp: u8) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"phoenix","card":"unit-follower"}]);
    initial
}

fn game(initial: &Value, tree: &Value, steps: Vec<Located<Node>>) -> Game {
    let programs = lowerer().lower(tree).unwrap();
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "cemetery-play",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![play::spell(steps)]))
        .unwrap();
    game
}

fn play(game: &mut Game) {
    resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    game.resume_internal_card().unwrap();
}

fn fire(game: &mut Game) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("pending")
    };
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn pay(game: &mut Game, play: CardPlay, accepted: bool) -> CardPlay {
    let CardPlay::Awaiting(payment) = play else {
        panic!("head payment")
    };
    assert!(
        matches!(payment.domain, InputDomain::OptionalCost),
        "the required head asks for its payment declaration"
    );
    game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(accepted))
        .unwrap()
}

#[test]
fn cemetery_play_matches_class_without_requiring_the_spells_title_registry() {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let titled_snapshot = snapshot
        .lines()
        .map(|line| {
            let mut entry: Value = serde_json::from_str(line).unwrap();
            if entry["number"] == "unit-spell" {
                entry["faces"][0]["title"] = json!("Unregistered synthetic title");
            }
            entry.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(
            &titled_snapshot,
            &registry,
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    );
    for observer_present in [false, true] {
        let mut initial = initial(2);
        if !observer_present {
            initial["players"]["P1"]["zones"]["cemetery"] = json!([]);
        }
        let programs = lowerer().lower(&tree()).unwrap();
        let mut game = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "cemetery-play-title",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![play::spell(vec![])]))
            .unwrap();
        play(&mut game);
        assert_eq!(
            game.internal_pending_choice().unwrap().is_some(),
            observer_present,
            "class-based Play subscription does not depend on an unrelated title registry"
        );
        if observer_present {
            let pending = fire(&mut game);
            resolved(pay(&mut game, pending, true));
            game.resume_internal_card().unwrap();
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["s"]),
            "the spell resolves normally"
        );
        assert_eq!(
            state["P1"]["field"],
            if observer_present {
                json!(["phoenix"])
            } else {
                json!([])
            },
            "the matching observer returns after its own payment"
        );
        assert_eq!(
            state["P1"]["pp"]["current"],
            u8::from(!observer_present),
            "only the spell and any declared return payment consume PP"
        );
    }
}

#[test]
fn cemetery_play_returns_the_observer_after_a_committed_spell_and_head_payment() {
    let mut game = game(&initial(2), &tree(), vec![]);
    play(&mut game);
    let pending = fire(&mut game);
    resolved(pay(&mut game, pending, true));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["phoenix"]));
    assert_eq!(state["P1"]["cemetery"], json!(["s"]));
    assert_eq!(state["P1"]["pp"]["current"], 0);
    assert_eq!(state["objects"]["phoenix"]["generation"], 1);
}

#[test]
fn cemetery_play_captures_before_the_spell_body_and_never_subscribes_from_field() {
    for initially_dead in [false, true] {
        let mut initial = initial(3);
        let mut steps = vec![];
        if !initially_dead {
            initial["players"]["P1"]["zones"]["cemetery"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"phoenix","card":"unit-follower"}]);
            steps.push(instruction(
                1,
                Action::Destroy {
                    objects: zone(ZoneKind::Field, 88),
                },
            ));
        }
        let mut game = game(&initial, &tree(), steps);
        play(&mut game);
        assert_eq!(
            game.internal_pending_choice().unwrap().is_some(),
            initially_dead
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"],
            json!(["phoenix", "s"])
        );
    }
}

#[test]
fn cemetery_play_distinguishes_class_controller_and_card_play_from_ability_play() {
    for case in 0_u8..4 {
        let mut initial = initial(3);
        let mut tree = tree();
        match case {
            0 => {
                tree["cards"]["unit-follower"]["abilities"][0]["auto"]["on"]["played"]["sel"]["class"] =
                    json!("ドラゴン");
            }
            1 => {
                initial["players"]["P1"]["zones"]["cemetery"] = json!([]);
                initial["players"]["P2"]["zones"]["cemetery"] =
                    json!([{"id":"phoenix","card":"unit-follower"}]);
            }
            2 => {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"caster","card":"unit-follower"}]);
                let mut activate = tree["cards"]["unit-follower"]["abilities"][0].clone();
                activate.as_object_mut().unwrap().remove("auto");
                activate["line"] = json!(2);
                activate["activate"] =
                    json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":1}}]});
                tree["cards"]["unit-follower"]["abilities"]
                    .as_array_mut()
                    .unwrap()
                    .push(activate);
            }
            _ => {
                initial["players"]["P1"]["zones"]["hand"] =
                    json!([{"id":"follower","card":"unit-follower"}]);
            }
        }
        let mut game = game(&initial, &tree, vec![]);
        match case {
            2 => {
                let programs = lowerer().lower(&tree).unwrap();
                resolved(
                    game.activate_internal_card(
                        "caster",
                        programs["unit-follower"].card().abilities[1].node.id,
                        PlayerId { seat: 1 },
                    )
                    .unwrap(),
                );
                game.resume_internal_card().unwrap();
            }
            3 => {
                assert!(matches!(
                    game.play_internal_card("follower", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ));
                game.resume_internal_card().unwrap();
            }
            _ => play(&mut game),
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn cemetery_play_each_copy_pays_independently_and_decline_does_not_return() {
    for accepted in [false, true] {
        let mut initial = initial(3);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"phoenix","card":"unit-follower"},{"id":"second","card":"unit-follower"}
        ]);
        let mut game = game(&initial, &tree(), vec![]);
        play(&mut game);
        for index in 0_u8..2 {
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = pending.domain else {
                panic!("pending")
            };
            assert_eq!(options.len(), usize::from(2_u8.saturating_sub(index)));
            let pending = fire(&mut game);
            let result = pay(&mut game, pending, accepted);
            if accepted {
                resolved(result);
            } else {
                assert!(matches!(result, CardPlay::Cancelled));
            }
            game.resume_internal_card().unwrap();
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"],
            if accepted {
                json!(["phoenix", "second"])
            } else {
                json!([])
            }
        );
        assert_eq!(state["P1"]["pp"]["current"], if accepted { 0 } else { 2 });
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn cemetery_play_insufficient_pp_cancels_and_full_field_records_no_success() {
    for full in [false, true] {
        let mut initial = initial(if full { 2 } else { 1 });
        if full {
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0_u8..5)
                    .map(|n| json!({"id":format!("field{n}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            );
        }
        let mut game = game(&initial, &tree(), vec![]);
        play(&mut game);
        let pending = fire(&mut game);
        let result = if matches!(pending, CardPlay::Awaiting(_)) {
            pay(&mut game, pending, true)
        } else {
            pending
        };
        if full {
            let returned = resolved(result);
            assert!(
                !returned
                    .progress
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, Fact::Movement(_)))
            );
            assert!(returned.progress.receipts.values().any(|outcome|matches!(outcome,InstructionOutcome::Move(outcome) if outcome.moved.iter().all(|member|member.after.is_none() && member.actual_to.is_none()))));
        } else {
            assert!(matches!(result, CardPlay::Cancelled));
        }
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["phoenix", "s"]));
        assert_eq!(state["P1"]["pp"]["current"], 0);
    }
}

#[test]
fn cemetery_play_keeps_other_zones_and_auto_headers_closed() {
    for change in 0_u8..4 {
        let mut tree = tree();
        let auto = &mut tree["cards"]["unit-follower"]["abilities"][0]["auto"];
        match change {
            0 => auto["zone"] = json!("ex"),
            1 => auto["on"]["played"]["during"]["turn"] = json!("opp"),
            2 => auto["on"]["played"]["sel"]["kind"] = json!("follower"),
            _ => {
                auto["do"][0]["move"]["ref"] =
                    json!({"who":"self","zone":"field","kind":"follower"});
            }
        }
        lowerer().lower(&tree).unwrap_err();
    }
    let programs = lowerer().lower(&tree()).unwrap();
    let mut mutated = programs["unit-follower"].card().clone();
    mutated.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .push(instruction(
            123,
            Action::Life {
                player: PlayerRef::Controller {},
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(1)),
                },
            },
        ));
    let span = mutated.abilities[0].source.clone();
    let origin = mutated.abilities[0].node.source.clone();
    locate_steps(
        &mut mutated.abilities[0].node.body.as_mut().unwrap().steps,
        &span,
        &origin,
    );
    let program = WellFormedCardProgram::check(mutated).unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(2),
        &Value::Null,
        &Value::Null,
        "closed-cemetery-play",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}

#[test]
fn cemetery_play_stale_pending_cannot_retrieve_a_returned_generation() {
    let mut tree = tree();
    let mut second = tree["cards"]["unit-follower"]["abilities"][0].clone();
    second["line"] = json!(2);
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let mut game = game(&initial(3), &tree, vec![]);
    play(&mut game);
    for attempt in 0_u8..2 {
        let pending = fire(&mut game);
        let result = pay(&mut game, pending, true);
        let returned = resolved(result);
        if attempt == 1 {
            assert!(
                !returned
                    .progress
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, Fact::Movement(_)))
            );
        }
        game.resume_internal_card().unwrap();
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["phoenix"]));
    assert_eq!(state["objects"]["phoenix"]["generation"], 1);
    assert_eq!(state["P1"]["pp"]["current"], 0);
}

#[test]
fn cemetery_play_opponent_turn_quick_spell_does_not_trigger_its_own_cemetery() {
    let mut initial = initial(3);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"attacker","card":"unit-follower","state":{"entered_this_turn":false}}]);
    initial["players"]["P2"]["zones"]["cemetery"] =
        json!([{"id":"phoenix","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"quick","card":"unit-spell"}]);
    let programs = lowerer().lower(&tree()).unwrap();
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "opponent-quick",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card(
        "unit-spell",
        &card(vec![play::spell(vec![]), quick::permit()]),
    )
    .unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"P2.leader"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(question.actor, PlayerId { seat: 2 });
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Quick(Some(0)))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["phoenix", "quick"]));
    assert_eq!(state["P2"]["pp"]["current"], 1);
    quick::pass(&mut game);
}

#[test]
fn cemetery_play_failed_spell_payment_never_enqueues_an_occurrence() {
    let mut game = game(&initial(0), &tree(), vec![]);
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
