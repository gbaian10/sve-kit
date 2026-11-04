#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the synthetic history scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use core::slice::from_ref;
use sve_engine::game::internal::InputDomain;

fn history() -> Value {
    json!({"window":"this_turn","event":"discarded","who":"self","from":"hand"})
}

fn tree(ability: &Value) -> Value {
    tree_with(from_ref(ability))
}

fn tree_with(abilities: &[Value]) -> Value {
    let mut source = fanfare::expanded::tree().clone();
    let meta = source["cards"]["BP08-038"]["meta"].clone();
    let template = source["cards"]["BP08-038"]["abilities"][0].clone();
    let entries = abilities
        .iter()
        .enumerate()
        .map(|(index, ability)| {
            let mut entry = template.clone();
            entry.as_object_mut().unwrap().remove("fanfare");
            entry["sources"][0]["ability_scope"] = json!(format!("discard_history_{index}"));
            for (key, value) in ability.as_object().unwrap() {
                entry[key] = value.clone();
            }
            entry
        })
        .collect::<Vec<_>>();
    source["cards"] = json!({"unit-follower":{"meta":meta,"abilities":entries}});
    source
}

fn read_history() -> Value {
    json!({"life":{"who":"self","delta":{"history_count":history()}}})
}

fn initial() -> Value {
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"h0","card":"unit-spell"},{"id":"h1","card":"unit-spell"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opponent-draw","card":"unit-spell"}]);
    initial
}

fn fixture(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "discard-history",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish_materials(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(vec![domain.members[0].handle])
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(vec![domain.members[0].handle])
        } else if question.domain == InputDomain::OptionalCost {
            Answer::OptionalCost(true)
        } else {
            panic!("only the declared hand selection may intervene")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn activate(game: &mut Game, ability: AbilityId) -> CardPlay {
    let play = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    let play = finish_materials(game, play);
    finish_checks(game);
    play
}

fn lower(abilities: &[Value]) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree_with(abilities))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn own_hand_discard_history_lowers_in_a_printed_end_phase_condition() {
    let source = tree(&json!({"auto":{
        "on":{"phase":{"which":"end","whose":"self"}},
        "do":[{"if":{"cond":{"history":history()},"then":[{"life":{"who":"self","delta":1}}]}}]
    }}));
    let program = lowerer()
        .lower(&source)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "discard-history",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
}

#[test]
fn own_hand_discard_history_lowers_in_a_printed_play_cost_condition() {
    let source = tree(&json!({"play":{"cost_delta":{"if":{"history":history()},"delta":-2}}}));
    let program = lowerer()
        .lower_with_printed_costs(
            &source,
            &BTreeMap::from([("unit-follower".to_owned(), Nat(3))]),
        )
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = Game::new(
        catalog_with_cost(&json!("3")),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "discard-discount",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
}

#[test]
fn discard_counts_actual_cards_but_draw_move_and_banish_do_not_count() {
    for (steps, expected) in [
        (
            json!([{"discard":{"who":"self","all":true}}, {"discard":{"who":"self","all":true}}]),
            2,
        ),
        (json!([{"draw":{"n":1}}]), 0),
        (
            json!([{"move":{"top":{"who":"self","n":1},"to":"cemetery"}}]),
            0,
        ),
        (
            json!([{"choose":{"bind":"chosen","from":{"who":"self","zone":"hand"},"n":1}}, {"banish":{"ref":"chosen"}}]),
            0,
        ),
    ] {
        let program = lower(&[
            json!({"activate":{"cost":{"pp":0},"do":steps}}),
            json!({"activate":{"cost":{"pp":0},"do":[read_history()]}}),
        ]);
        let mut game = fixture(&initial(), &program);
        resolved(activate(&mut game, program.card().abilities[0].node.id));
        resolved(activate(&mut game, program.card().abilities[1].node.id));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20 + expected
        );
    }
}

#[test]
fn an_opponents_discard_does_not_count_as_the_effect_controllers_discard() {
    let program = lower(&[
        json!({"activate":{"cost":{"pp":0},"do":[{"discard":{"who":"opp","all":true}}]}}),
        json!({"activate":{"cost":{"pp":0},"do":[read_history()]}}),
    ]);
    let mut initial = initial();
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"opp-h0","card":"unit-spell"},{"id":"opp-h1","card":"unit-spell"}
    ]);
    let mut game = fixture(&initial, &program);
    resolved(activate(&mut game, program.card().abilities[0].node.id));
    resolved(activate(&mut game, program.card().abilities[1].node.id));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20);
    assert_eq!(state["P2"]["cemetery"], json!(["opp-h0", "opp-h1"]));
}

#[test]
fn paid_discard_counts_but_a_later_payment_failure_rolls_it_back() {
    let program = lower(&[
        json!({"activate":{"cost":{"steps":[{"discard":{"sel":{"who":"self","zone":"hand"},"n":1}},{"pp":3}]},"do":[]}}),
        json!({"activate":{"cost":{"pp":0},"do":[read_history()]}}),
    ]);
    for pp in [2, 4] {
        let mut initial = initial();
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
        let mut game = fixture(&initial, &program);
        let before = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("private discard payment")
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("private hand materials")
        };
        let result = game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        );
        if pp == 2 {
            result.unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        } else {
            resolved(result.unwrap());
        }
        finish_checks(&mut game);
        resolved(activate(&mut game, program.card().abilities[1].node.id));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], if pp == 2 { 20 } else { 21 });
        assert_eq!(state["P1"]["pp"]["current"], if pp == 2 { 2 } else { 1 });
        assert_eq!(
            state["P1"]["hand"].as_array().unwrap().len(),
            if pp == 2 { 2 } else { 1 }
        );
    }
}

#[test]
fn explicit_discard_history_is_additive_and_resets_at_the_next_turn() {
    let program = lower(&[
        json!({"activate":{"cost":{"pp":0},"do":[{"discard":{"who":"self","all":true}}]}}),
        json!({"activate":{"cost":{"pp":0},"do":[read_history()]}}),
    ]);
    let mut initial = initial();
    initial["semantic_state"]["counters_this_turn"] = json!({"P1":{"discarded":3}});
    let mut game = fixture(&initial, &program);
    let reader = program.card().abilities[1].node.id;
    resolved(activate(&mut game, reader));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23
    );
    resolved(activate(&mut game, program.card().abilities[0].node.id));
    resolved(activate(&mut game, reader));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        28
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    resolved(
        game.activate_internal_card("f", reader, PlayerId { seat: 2 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20
    );
}

#[test]
fn a_rule_hand_limit_discard_is_committed_before_its_observer_reads_history() {
    let program = lower(&[json!({"auto":{
        "on":{"discarded":{"who":"self"}},"do":[read_history()]
    }})]);
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["hand"] = json!(
        (0..8)
            .map(|i| json!({"id":format!("h{i}"),"card":"unit-spell"}))
            .collect::<Vec<_>>()
    );
    let mut game = fixture(&initial, &program);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    game.internal_rule_action(&json!({"do":"end-discard","select":["h7"]}))
        .unwrap();
    finish_checks(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("rule discard observation")
    };
    assert_eq!(options.len(), 1);
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    resolved(play);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21
    );
}

#[test]
fn discard_history_keeps_both_query_sites_closed_and_does_not_widen_moved() {
    let mut queries = Vec::new();
    for (key, value) in [
        ("who", json!("opp")),
        ("from", json!("field")),
        ("window", json!("game")),
        ("sel", json!({"kind":"follower"})),
        ("distinct", json!("subjects")),
        ("exclude", json!("this_execution")),
        ("where", json!({"ref":"this"})),
        ("to", json!("cemetery")),
        ("event", json!("moved")),
    ] {
        let mut query = history();
        query[key] = value;
        queries.push(query);
    }
    for missing in ["who", "from"] {
        let mut query = history();
        query.as_object_mut().unwrap().remove(missing);
        queries.push(query);
    }
    for query in queries {
        for cost in [false, true] {
            let ability = if cost {
                json!({"play":{"cost_delta":{"if":{"history":query},"delta":-2}}})
            } else {
                json!({"activate":{"cost":{"pp":0},"do":[{"life":{"delta":{"history_count":query},"who":"self"}}]}})
            };
            lowerer()
                .lower_with_printed_costs(
                    &tree(&ability),
                    &BTreeMap::from([("unit-follower".to_owned(), Nat(3))]),
                )
                .unwrap_err();
        }
    }
    for query in [
        json!({"window":"this_turn","event":"played","who":"self"}),
        json!({"window":"this_turn","event":"life_changed","who":"self","where":{"direction":"down"}}),
    ] {
        let source = tree(&json!({"play":{"cost_delta":{"if":{"history":query},"delta":-2}}}));
        lowerer()
            .lower_with_printed_costs(
                &source,
                &BTreeMap::from([("unit-follower".to_owned(), Nat(3))]),
            )
            .unwrap_err();
    }
}

fn history_card(query: HistoryQuery, count: HistoryCount) -> CardProgram {
    CardProgram {
        abilities: vec![payment::paid_ability(
            vec![],
            vec![instruction(
                1,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Compute(Box::new(IntOperation::FromNat {
                            value: Expr::Live(Box::new(NatRead::HistoryCount { query, count })),
                        })),
                    },
                },
            )],
        )],
        card_play_costs: vec![],
    }
}

#[test]
fn raw_ir_discard_history_rejects_subject_counts_and_noncanonical_windows_or_filters() {
    let event = Binding::new(900, ScopeId(1));
    let query = HistoryQuery {
        pattern: EventPattern::Discard {
            binding: event,
            predicate: Expr::Compute(Box::new(Predicate::And {
                predicates: vec![Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Frozen(Box::new(PlayerFrozen::DiscardPlayer { event })),
                    right: Expr::Literal(PlayerRef::Controller {}),
                }))],
            })),
        },
        window: HistoryWindow {
            range: HistoryRange::CurrentTurn {},
            cutoff: HistoryCutoff::CurrentPoint {},
            relative_player: None,
        },
        include_execution: InclusionPolicy::IncludeAll {},
    };
    WellFormedCardProgram::check(history_card(query.clone(), HistoryCount::Facts)).unwrap();
    WellFormedCardProgram::check(history_card(query.clone(), HistoryCount::Subjects)).unwrap_err();
    let mut window = query.clone();
    window.window.relative_player = Some(PlayerRef::Active {});
    let mut inclusion = query.clone();
    inclusion.include_execution = InclusionPolicy::ExcludeCurrentExecution {
        execution: ExecutionRef::Current {},
    };
    let mut unfiltered = query;
    if let EventPattern::Discard { predicate, .. } = &mut unfiltered.pattern {
        *predicate = Expr::Literal(Bool(true));
    }
    for invalid in [window, inclusion, unfiltered] {
        WellFormedCardProgram::check(history_card(invalid, HistoryCount::Facts)).unwrap_err();
    }
}
