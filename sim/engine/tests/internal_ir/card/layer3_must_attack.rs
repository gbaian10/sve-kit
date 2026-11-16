use super::*;
use sve_engine::game::internal::InputDomain;

fn input() -> Value {
    evolution_linked_movement::tree(&[(
        "static",
        json!({
            "while":{"not":{"standing":"this"}},
            "effects":[{"must_attack":{"on":{"who":"opp","zone":"field","kind":"follower"}}}]
        }),
    )])
}

fn installation() -> Value {
    evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"grant":{
            "on":"this","until":{"during_next_turn":{"who":"opp"}},
            "abilities":[{"static":{
                "while":{"not":{"standing":"this"}},
                "effects":[{"must_attack":{"on":{"who":"opp","zone":"field","kind":"follower"}}}]
            }}]
        }}]}),
    )])
}

#[test]
fn a_granted_requirement_applies_to_only_one_opponent_turn() {
    let mut value = installation();
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["grant"]["abilities"][0]["static"].as_object_mut().unwrap().remove("while");
    let extra =
        evolution_linked_movement::tree(&[("activate", json!({"cost":{"act_self":true},"do":[]}))]);
    let mut act = extra["cards"]["unit-follower"]["abilities"][0].clone();
    act["line"] = json!(2);
    value["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(act);
    let programs = lowerer().lower(&value).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    for side in ["P1", "P2"] {
        initial["players"][side]["zones"]["hand"] = json!([]);
        initial["players"][side]["zones"]["deck"] = json!(
            (0..4)
                .map(|n| json!({"id":format!("{side}-{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"unit-follower"}]);
    let mut board = evolution_linked_movement::loaded(&initial, program);
    resolved(
        board
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
    );
    event_ability::finish(&mut board);
    end(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "cannot-play"
    );
    // Acting for another ability remains legal even before the required attack.
    resolved(
        board
            .activate_internal_card(
                "attacker",
                program.card().abilities[1].node.id,
                PlayerId { seat: 2 },
            )
            .unwrap(),
    );
    event_ability::finish(&mut board);
    end(&mut board);
    end(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["attacker"]["acted"],
        false
    );
    end(&mut board);
}

#[test]
fn an_attack_requirement_prevents_ending_main_only_while_a_legal_unfulfilled_attack_exists() {
    let program = lowerer()
        .lower(&input())
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"unit-follower"}]);
    let mut board = evolution_linked_movement::loaded(&initial, &program);
    let before = board.projection(View::Referee).unwrap();
    let outcome = board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    assert_eq!(outcome.outcome, "cannot-play");
    assert_eq!(board.projection(View::Referee).unwrap(), before);
}

#[test]
fn a_requirement_uses_current_posture_abilities_attack_count_and_formal_prohibitions() {
    for case in [
        "required",
        "standing",
        "silenced",
        "acted",
        "attacked",
        "prohibited",
        "newcomer",
    ] {
        let mut value = input();
        if case == "prohibited" {
            let extra = evolution_linked_movement::tree(&[(
                "static",
                json!({"effects":[{"cannot":{"what":"attack","on":"this"}}]}),
            )]);
            let mut ability = extra["cards"]["unit-follower"]["abilities"][0].clone();
            ability["line"] = json!(2);
            value["cards"]["unit-follower"]["abilities"]
                .as_array_mut()
                .unwrap()
                .push(ability);
        }
        let program = lowerer()
            .lower(&value)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let mut initial = setup(1, json!([]));
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{
            "id":"s","card":"unit-follower","state":{"acted":case!="standing","silenced":case=="silenced"}
        }]);
        initial["players"]["P2"]["zones"]["field"] = json!([{
            "id":"attacker","card":"unit-follower","state":{
                "acted":case=="acted","attacks_this_turn":u8::from(case=="attacked"),
                "entered_this_turn":case=="newcomer"
            }
        }]);
        let mut board = evolution_linked_movement::loaded(&initial, &program);
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            if case == "required" {
                "cannot-play"
            } else {
                "resolved"
            },
            "case {case}"
        );
    }
}

fn end(board: &mut Game) {
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the synthetic position has no attack requirement"
    );
    event_ability::finish(board);
    quick::pass(board);
    event_ability::finish(board);
    board
        .internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    event_ability::finish(board);
}

#[test]
fn next_turn_keyword_grants_wait_through_extra_and_skipped_turns_then_expire() {
    let value = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"grant":{"on":"this","keywords":["bane"],"until":{"during_next_turn":{"who":"opp"}}}}
        ]}),
    )]);
    let programs = lowerer().lower(&value).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["semantic_state"]["turn_schedule"] =
        json!({"extra":[{"player":"P1","count":1}],"skipped":[{"player":"P2","count":1}]});
    for side in ["P1", "P2"] {
        initial["players"][side]["zones"]["hand"] = json!([]);
        initial["players"][side]["zones"]["deck"] = json!(
            (0..6)
                .map(|n| json!({"id":format!("{side}-{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let (snapshot, _, document) = catalog_documents(&json!("1"));
    let registry =
        json!({"format":1,"kind":"keyword_registry","keywords":{"bane":{"ja":"必殺","rule":"12.14","expansion":{"op":"keyword","name":"bane"}}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut board = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "next-turn-grant",
    )
    .unwrap();
    board.load_internal_card("unit-follower", program).unwrap();
    resolved(
        board
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
    );
    event_ability::finish(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
        json!([])
    );
    for _ in 0..2 {
        end(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["turn"]["active"], "P1");
        assert_eq!(view["objects"]["s"]["keywords"], json!([]));
    }
    end(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["turn"]["active"], "P2");
    assert_eq!(view["objects"]["s"]["keywords"], json!(["必殺"]));
    end(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
        json!([])
    );
}

#[test]
fn changing_the_provider_posture_or_abilities_removes_a_live_requirement() {
    for effect in [
        json!({"stand":"provider"}),
        json!({"apply":{"on":"provider","effect":{"lose_abilities":{}},"until":"turn"}}),
        json!({"move":{"ref":"provider","to":"hand"}}),
    ] {
        let mut value = input();
        let extra = evolution_linked_movement::tree(&[(
            "activate",
            json!({"cost":{"pp":0},
                "targets":[{"bind":"provider","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
                "do":[effect]
            }),
        )]);
        let mut ability = extra["cards"]["unit-follower"]["abilities"][0].clone();
        ability["line"] = json!(2);
        value["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(ability);
        let programs = lowerer().lower(&value).unwrap();
        let program = &programs["unit-follower"];
        let mut initial = setup(1, json!([]));
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower","state":{"acted":true}}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"attacker","card":"unit-follower"}]);
        let mut board = evolution_linked_movement::loaded(&initial, program);
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "cannot-play"
        );
        let CardPlay::Awaiting(q) = board
            .activate_internal_card(
                "attacker",
                program.card().abilities[1].node.id,
                PlayerId { seat: 2 },
            )
            .unwrap()
        else {
            panic!("provider target");
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("formal target");
        };
        resolved(
            board
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        event_ability::finish(&mut board);
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
    }
}

#[test]
fn grant_intervals_must_match_their_expiry_player_and_have_one_supported_interval() {
    let value = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"grant":{"on":"this","keywords":["bane"],"until":{"during_next_turn":{"who":"opp"}}}}
        ]}),
    )]);
    let programs = lowerer().lower(&value).unwrap();
    for case in ["different-player", "no-expiry", "repeated-interval"] {
        let mut raw = programs["unit-follower"].card().clone();
        let Node::Instruction { instruction } =
            &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("grant instruction");
        };
        let Action::Grant { duration, .. } = &mut instruction.action else {
            panic!("grant action");
        };
        match case {
            "different-player" => {
                duration.end = DurationEnd::NextTurnEnd {
                    player: PlayerRef::Controller {},
                }
            }
            "no-expiry" => duration.end = DurationEnd::NoTemporalExpiry {},
            _ => duration.intervals.push(duration.intervals[0].clone()),
        }
        assert!(WellFormedCardProgram::check(raw).is_err(), "{case}");
    }
}

#[test]
fn a_new_storm_follower_and_its_replayed_generation_each_owe_a_new_attack() {
    let mut value = input();
    for (line, head, body) in [
        (
            2,
            "static",
            json!({"effects":[{"grant":{"on":"this","keywords":["storm"]}}]}),
        ),
        (
            3,
            "activate",
            json!({"cost":{"pp":0},"do":[{"move":{"ref":"this","to":"hand"}}]}),
        ),
    ] {
        let extra = evolution_linked_movement::tree(&[(head, body)]);
        let mut ability = extra["cards"]["unit-follower"]["abilities"][0].clone();
        ability["line"] = json!(line);
        value["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(ability);
    }
    let programs = lowerer().lower(&value).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"old","card":"unit-follower","state":{"attacks_this_turn":1}}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"fresh","card":"unit-follower"}]);
    let mut board = evolution_linked_movement::loaded(&initial, program);
    assert!(matches!(
        board
            .play_internal_card("fresh", PlayerId { seat: 2 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    event_ability::finish(&mut board);
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "cannot-play"
    );
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"fresh","target":"P1.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    event_ability::finish(&mut board);
    quick::pass(&mut board);
    event_ability::finish(&mut board);
    resolved(
        board
            .activate_internal_card(
                "fresh",
                program.card().abilities[2].node.id,
                PlayerId { seat: 2 },
            )
            .unwrap(),
    );
    event_ability::finish(&mut board);
    assert!(matches!(
        board
            .play_internal_card("fresh", PlayerId { seat: 2 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    event_ability::finish(&mut board);
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "cannot-play"
    );
}
