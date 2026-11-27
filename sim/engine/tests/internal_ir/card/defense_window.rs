use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{EffectExpiry, ObservedZone};

fn until_next_own_turn() -> Value {
    json!({"next_turn":{"who":"self","edge":"begin"}})
}

fn protection() -> Value {
    json!({"player_effect":{
        "who":"self",
        "effect":{"cannot":{"what":"lose_defense"}},
        "until":until_next_own_turn()
    }})
}

fn window_program() -> WellFormedCardProgram {
    let input = evolution_linked_movement::tree(&[
        ("lastword", json!({"do":[protection()]})),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[
                {"damage":{"to":{"leader":"self"},"n":3}}, {"destroy":"this"}
            ]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[
                {"damage":{"to":{"leader":"self"},"n":1}},
                {"life":{"who":"self","delta":-2}},
                {"set_life":{"who":"self","n":6}},
                {"life":{"who":"self","delta":1}},
                {"set_life":{"who":"self","n":21}},
                {"draw":{"n":{"history_count":{"event":"life_changed","window":"this_turn","who":"self","where":{"direction":"down"}}}}},
                {"damage":{"to":{"leader":"opp"},"n":2}}
            ]}),
        ),
        (
            "activate",
            json!({"cost":{"steps":[{"pp":1},{"leader_life":2}]},"do":[
                {"life":{"who":"self","delta":1}}
            ]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[
                {"grant":{"on":{"who":"self","zone":"field","kind":"follower"},
                    "keywords":["intimidate","aura"],"until":until_next_own_turn()}}
            ]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[
                {"grant":{"on":"this","keywords":["drain"]}}
            ]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"damage":{"to":{"leader":"opp"},"n":2}}]}),
        ),
    ]);
    boxed::program(&input)
}

fn turn_schedules() -> [(Value, usize); 3] {
    [
        (Value::Null, 1),
        (json!({"extra":[{"player":"P1","count":1}],"skipped":[]}), 0),
        (json!({"extra":[],"skipped":[{"player":"P1","count":1}]}), 2),
    ]
}

fn board(program: &WellFormedCardProgram, schedule: Value) -> Game {
    let mut initial = setup(1, json!([]));
    if !schedule.is_null() {
        initial["semantic_state"]["turn_schedule"] = schedule;
    }
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"unit-follower"},
        {"id":"s","card":"unit-follower"},
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"f","card":"unit-follower"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["hand"] = json!([]);
        initial["players"][seat]["pp"] = json!({"max":3,"current":3});
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..8)
                .map(|index| json!({"id":format!("{seat}-{index}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let (snapshot, keywords, index) = catalog_documents(&json!("1"));
    let mut registry: Value = serde_json::from_str(&keywords).unwrap();
    for keyword in ["aura", "intimidate", "drain"] {
        registry["rule_names"][keyword] = json!({"ja":keyword,"rule":"12"});
    }
    let catalog = Arc::new(
        Catalog::from_fixture_documents(
            &snapshot,
            &registry.to_string(),
            &[("defense-fixture".into(), index)],
        )
        .unwrap(),
    );
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "defense-window",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn activate_line(
    game: &mut Game,
    program: &WellFormedCardProgram,
    source: &str,
    seat: u32,
    line: usize,
) -> Activation {
    let result = resolved(
        game.activate_internal_card(
            source,
            program.card().abilities[line.checked_sub(1).unwrap()]
                .node
                .id,
            PlayerId { seat },
        )
        .unwrap(),
    );
    finish_timing(game);
    result
}

fn finish_timing(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn install_lastword(game: &mut Game, program: &WellFormedCardProgram) {
    let before_protection = activate_line(game, program, "source", 1, 2);
    assert!(
        before_protection
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Damage(hit) if hit.amount.0.get() == 3)),
        "the destruction ability deals damage before Last Words resolves"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 17_u64,
        "only the pre-Last-Words damage applies"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["source"]),
        "the departed source is in the cemetery"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("destroying the source queues its Last Words");
    };
    assert_eq!(options.len(), 1, "only the source Last Words is pending");
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish_timing(game);
}

#[test]
fn leader_protection_prevents_damage_and_direct_reductions_but_allows_increases() {
    let program = window_program();
    let mut game = board(&program, Value::Null);
    install_lastword(&mut game, &program);
    let protected = activate_line(&mut game, &program, "s", 1, 3);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u64);
    assert_eq!(state["P2"]["leader"]["life"], 18_u64);
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        1,
        "only the pre-Last-Words damage counts as life lost"
    );
    let damage = protected
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        matches!(damage.as_slice(), [hit] if hit.recipient == (RuntimeDamageRecipient::Leader { player: PlayerId { seat: 2 } }) && hit.amount.0.get() == 2)
    );
    let life = protected
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::LifeChange(change) = fact {
                Some(change.delta.0)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        life,
        [1, 3],
        "the protected leader only records its two increases"
    );
}

#[test]
fn leader_protection_rejects_required_life_payment_until_the_next_own_turn() {
    let program = window_program();
    for (schedule, opponent_turns) in turn_schedules() {
        let mut game = board(&program, schedule);
        install_lastword(&mut game, &program);
        let before = exact_authority(&game);
        let error = game
            .activate_internal_card(
                "s",
                program.card().abilities[3].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap_err();
        assert!(format!("{error:?}").contains("leader cannot lose defense"));
        assert_eq!(
            exact_authority(&game),
            before,
            "no resource or event is committed when the cost cannot be paid"
        );
        boxed::end_turn(&mut game);
        for _ in 0..opponent_turns {
            assert_eq!(
                game.projection(View::Referee).unwrap()["turn"]["active"],
                "P2"
            );
            let damage = activate_line(&mut game, &program, "f", 2, 7);
            assert!(
                damage.progress.facts.iter().all(|fact| !matches!(
                    fact, Fact::Damage(hit) if hit.recipient == (RuntimeDamageRecipient::Leader { player: PlayerId { seat: 1 } })
                ))
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                17_u64
            );
            boxed::end_turn(&mut game);
        }
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["turn"]["active"], "P1");
        activate_line(&mut game, &program, "s", 1, 4);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 16_u64);
        assert_eq!(
            after["P1"]["pp"]["current"].as_i64().unwrap(),
            before["P1"]["pp"]["current"]
                .as_i64()
                .unwrap()
                .checked_sub(1)
                .unwrap()
        );
    }
}

#[test]
fn a_drain_attack_against_a_protected_leader_produces_no_damage_or_healing() {
    let program = window_program();
    let mut game = board(&program, Value::Null);
    install_lastword(&mut game, &program);
    boxed::end_turn(&mut game);
    activate_line(&mut game, &program, "f", 2, 6);
    game.internal_rule_action(&json!({"do":"attack","attacker":"f","target":"P1.leader"}))
        .unwrap();
    quick::pass(&mut game);
    finish_timing(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["f"]["acted"], true);
    assert_eq!(
        state["P1"]["leader"]["life"], 17_u64,
        "only the pre-Last-Words damage applies"
    );
    assert_eq!(state["P2"]["leader"]["life"], 20_u64);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "Drain requires actual damage"
    );
}

#[test]
fn next_own_turn_start_grants_follow_normal_extra_and_skipped_turns() {
    let program = window_program();
    for (schedule, opponent_turns) in turn_schedules() {
        let mut game = board(&program, schedule);
        activate_line(&mut game, &program, "s", 1, 5);
        let state = game.projection(View::Referee).unwrap();
        for id in ["s", "source"] {
            assert_eq!(
                state["objects"][id]["keywords"],
                json!(["aura", "intimidate"])
            );
        }
        let observed = game.observe(Viewer::P1).unwrap();
        let field = observed.players[0]
            .zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Field)
            .unwrap();
        assert_eq!(field.cards.len(), 2);
        assert!(field.cards.iter().all(|member| {
            member
                .card
                .properties
                .state
                .as_ref()
                .unwrap()
                .effects
                .iter()
                .any(|effect| {
                    effect.active && effect.expires == EffectExpiry::NextTurnStart(Viewer::P1)
                })
        }));
        boxed::end_turn(&mut game);
        for _ in 0..opponent_turns {
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["turn"]["active"], "P2");
            assert_eq!(
                state["objects"]["s"]["keywords"],
                json!(["aura", "intimidate"])
            );
            boxed::end_turn(&mut game);
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["turn"]["active"], "P1");
        assert_eq!(state["objects"]["s"]["keywords"], json!([]));
        assert_eq!(state["objects"]["source"]["keywords"], json!([]));
    }
}

#[test]
fn next_turn_start_authoring_rejects_unadmitted_rules_and_edges() {
    for (who, until) in [
        ("opp", until_next_own_turn()),
        ("self", json!("turn")),
        ("self", json!({"next_turn":{"who":"opp","edge":"begin"}})),
        ("self", json!({"next_turn":{"who":"self","edge":"end"}})),
    ] {
        let input = evolution_linked_movement::tree(&[(
            "lastword",
            json!({"do":[
                {"player_effect":{"who":who,"effect":{"cannot":{"what":"lose_defense"}},"until":until}}
            ]}),
        )]);
        lowerer().lower(&input).unwrap_err();
    }
    for until in [
        json!({"next_turn":{"who":"opp","edge":"begin"}}),
        json!({"next_turn":{"who":"self","edge":"end"}}),
    ] {
        let input = evolution_linked_movement::tree(&[(
            "fanfare",
            json!({"do":[
                {"grant":{"on":"this","keywords":["aura"],"until":until}}
            ]}),
        )]);
        lowerer().lower(&input).unwrap_err();
    }
    let input = evolution_linked_movement::tree(&[("lastword", json!({"do":[protection()]}))]);
    player_hand_limit::assert_player_rule_rejects_mutations(&boxed::program(&input));
}
