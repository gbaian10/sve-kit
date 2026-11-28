use super::*;

fn tree(on: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["auto"] = json!({"on":on,"do":[{"buff":{"on":"this","power":1,"hp":1}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

#[test]
fn own_non_start_draw_lowers_to_per_fact_after_entry_on_the_existing_draw_event() {
    let programs = lowerer()
        .lower(&tree(&json!({"drew":{"who":"self","not_phase":"start"}})))
        .unwrap();
    let card = programs["unit-follower"].card();
    let trigger = card.abilities[0].node.trigger.as_ref().unwrap();
    assert!(matches!(
        trigger.on,
        TriggerCondition::Event {
            pattern: EventPattern::Draw { .. }
        }
    ));
    assert_eq!(trigger.view, EventSnapshotPolicy::AfterEntry);
    assert_eq!(trigger.aggregation, TriggerAggregation::PerFact {});
}

#[test]
fn draw_headers_keep_other_players_phases_filters_and_missing_exclusions_closed() {
    for payload in [
        json!({"who":"opp","not_phase":"start"}),
        json!({"who":"self","not_phase":"main"}),
        json!({"who":"self","not_phase":"end"}),
        json!({"who":"self"}),
        json!({"who":"all","not_phase":"start"}),
        json!({"who":"all","by":"ability"}),
        json!({"who":"opp"}),
        json!({"not_phase":"start"}),
        json!({"who":"self","not_phase":"start","by":"ability"}),
    ] {
        lowerer()
            .lower(&tree(&json!({"drew":payload})))
            .unwrap_err();
    }
}

#[test]
fn every_player_draw_header_counts_each_successful_draw_including_turn_start() {
    let program = boxed::program(&evolution_linked_movement::tree(&[
        (
            "auto",
            json!({"on":{"drew":{"who":"all"}},"do":[{"buff":{"on":"this","power":1}}]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"each_player":{"as":"p","stages":[[
                {"draw":{"who":"p","n":2}}
            ]]}}]}),
        ),
    ]));
    let mut initial = setup(1, json!([]));
    for (seat, observer) in [("P1", "observer-one"), ("P2", "observer-two")] {
        initial["players"][seat]["zones"]["hand"] = json!([]);
        initial["players"][seat]["zones"]["field"] =
            json!([{"id":observer,"card":"unit-follower"}]);
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..4)
                .map(|index| json!({"id":format!("{seat}-draw-{index}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let ability = program.card().abilities[1].node.id;
    resolved(
        game.activate_internal_card("observer-one", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let actors = resolve_draw_observers(&mut game, 8);
    assert_eq!(
        actors.iter().filter(|&&seat| seat == 1).count(),
        4,
        "the first observer sees all four draws"
    );
    assert_eq!(
        actors.iter().filter(|&&seat| seat == 2).count(),
        4,
        "the second observer sees all four draws"
    );
    let after_effect = game.projection(View::Referee).unwrap();
    for observer in ["observer-one", "observer-two"] {
        assert_eq!(
            after_effect["objects"][observer]["power"], 6,
            "each observer resolves once for every drawn card"
        );
    }

    boxed::end_turn(&mut game);
    let actors = resolve_draw_observers(&mut game, 2);
    assert_eq!(
        actors,
        [2, 1],
        "turn-player order applies to both observers of the start draw"
    );
    let after_start = game.projection(View::Referee).unwrap();
    assert_eq!(
        after_start["P2"]["hand"].as_array().unwrap().len(),
        3,
        "the rule draws one more card"
    );
    for observer in ["observer-one", "observer-two"] {
        assert_eq!(
            after_start["objects"][observer]["power"], 7,
            "the start draw also produces one occurrence per observer"
        );
    }
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "there are no duplicate draw occurrences"
    );
}

fn resolve_draw_observers(game: &mut Game, count: usize) -> Vec<u32> {
    use sve_engine::game::internal::InputDomain;
    let mut actors = Vec::new();
    for _ in 0..count {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("a committed draw queues a printed observer");
        };
        actors.push(pending.actor.seat);
        resolved(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    actors
}
