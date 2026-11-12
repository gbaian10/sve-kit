use super::*;
use sve_engine::game::internal::ResumeStage;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::TurnHistory;

fn snapshot(game: &Game) -> (Value, TurnHistory) {
    (
        game.projection(View::Referee).unwrap(),
        game.observe(Viewer::P1).unwrap().history,
    )
}

fn consumer(query: &Value, qualify_evolution: bool, destination: &str) -> WellFormedCardProgram {
    let mut tree = equipment_played::tree(
        &json!({"play":{
            "cost_delta":{"if":{"history":query},"delta":-1}
        }}),
        "unit-follower",
    );
    let mut evolution = json!({"evolve":{"cost":{"pp":1}}});
    if qualify_evolution {
        evolution["evolve"]["usable_if"] = json!({"history":query});
    }
    let departure = if destination == "banish" {
        json!({"banish":"this"})
    } else {
        json!({"move":{"ref":"this","to":destination}})
    };
    for (index, head) in [
        evolution,
        json!({"activate":{"cost":{"pp":0},"do":[{"if":{
            "cond":{"history":query},"then":[{"life":{"who":"self","delta":1}}]
        }}]}}),
        json!({"activate":{"cost":{"pp":0},"do":[departure]}}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut entry =
            equipment_played::tree(&head, "unit-follower")["cards"]["unit-follower"]["abilities"]
                [0]
            .clone();
        entry["line"] = json!(index.checked_add(1).unwrap());
        tree["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(entry);
    }
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        } else if let Some(question) = game.internal_quick_choice().unwrap() {
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
        } else {
            break;
        }
    }
}

fn departure_form() -> WellFormedCardProgram {
    lowerer()
        .lower(&equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},
        "do":[{"move":{"ref":"this","to":"cemetery"}}]}}),
            "unit-form",
        ))
        .unwrap()
        .remove("unit-form")
        .unwrap()
}

fn depart(game: &mut Game, holder: &str, definition: AbilityId) {
    game.activate_internal_card(holder, definition, PlayerId { seat: 1 })
        .unwrap();
    settle(game);
}

fn initial_game(
    base: &WellFormedCardProgram,
    form: &WellFormedCardProgram,
    subjects: usize,
) -> Game {
    let mut field = vec![json!({"id":"s","card":"unit-follower"})];
    field.extend(
        (0..subjects).map(|index| json!({"id":format!("u{index}"),"card":"unit-follower"})),
    );
    evolution::position_with_zones(
        1,
        base,
        form,
        false,
        &json!({"P1":{"field":field,"hand":[{"id":"discount","card":"unit-follower"}]}}),
    )
}

#[test]
fn committed_subject_facts_share_cost_evolution_and_body_conditions_after_departure() {
    for event in ["attacked", "left_field"] {
        let threshold = if event == "attacked" { 3 } else { 1 };
        let query = json!({"event":event,"window":"this_turn","who":"self",
            "sel":{"kind":"follower","name":"unit-follower"},"at_least":threshold});
        let program = consumer(&query, true, "cemetery");
        for qualifying in [false, true] {
            let subjects = if event == "attacked" {
                2_usize.checked_add(usize::from(qualifying)).unwrap()
            } else {
                usize::from(qualifying)
            };
            let mut game = initial_game(&program, &card(vec![]), subjects);
            for index in 0..subjects {
                let holder = format!("u{index}");
                if event == "attacked" {
                    game.internal_rule_action(
                        &json!({"do":"attack","attacker":holder,"target":"P2.leader"}),
                    )
                    .unwrap();
                    settle(&mut game);
                }
                depart(&mut game, &holder, program.card().abilities[2].node.id);
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .len(),
                subjects
            );
            let before = snapshot(&game);
            let expected = i64::from(!qualifying);
            assert_eq!(
                cost_observation::preview(&game, Viewer::P1).cost.total_pp,
                Some(expected)
            );
            assert_eq!(
                snapshot(&game),
                before,
                "quoting preserves historical facts"
            );
            let life = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
                .as_u64()
                .unwrap();
            game.activate_internal_card(
                "s",
                program.card().abilities[1].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap();
            settle(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                life + u64::from(qualifying)
            );
            let before = snapshot(&game);
            let evolved = game
                .internal_rule_action(&json!({"do":"evolve","source":"s","evolve_card":"e",
                "definition":program.card().abilities[0].node.id.0,"pay":{"pp":1}}))
                .unwrap();
            assert_eq!(
                evolved.outcome,
                if qualifying {
                    "resolved"
                } else {
                    "cannot-evolve"
                }
            );
            if !qualifying {
                assert_eq!(snapshot(&game), before);
            }
            settle(&mut game);
            let pp = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
                .as_i64()
                .unwrap();
            game.play_internal_card("discount", PlayerId { seat: 1 })
                .unwrap();
            settle(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                pp - expected
            );
        }
    }
}

#[test]
fn committed_evolution_facts_still_discount_after_the_evolved_subject_leaves() {
    let query = json!({"event":"evolved","window":"this_turn","who":"self","at_least":1});
    let program = consumer(&query, false, "cemetery");
    let form = departure_form();
    let mut game = initial_game(&program, &form, 1);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"u0","evolve_card":"e","pay":{"pp":1}}),
    )
    .unwrap();
    settle(&mut game);
    depart(&mut game, "u0", form.card().abilities[0].node.id);
    assert_eq!(
        cost_observation::preview(&game, Viewer::P1).cost.total_pp,
        Some(0)
    );
    let pp = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"].clone();
    game.play_internal_card("discount", PlayerId { seat: 1 })
        .unwrap();
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        pp
    );
}

#[test]
fn played_intersection_qualifies_evolution_and_survives_the_evolution() {
    let query = json!({"event":"played","window":"this_turn","who":"self",
        "sel":{"kind":"follower","not_token":true}});
    let input = equipment_played::tree(
        &json!({"evolve":{"cost":{"pp":1},
        "usable_if":{"history":query}}}),
        "unit-follower",
    );
    let base = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let input = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},
        "do":[{"life":{"who":"self","delta":{"history_count":query}}}]}}),
        "unit-form",
    );
    let form = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-form")
        .unwrap();
    let mut game = initial_game(&base, &form, 1);
    let before = snapshot(&game);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":1}})
        )
        .unwrap()
        .outcome,
        "cannot-evolve"
    );
    assert_eq!(snapshot(&game), before);
    game.play_internal_card("discount", PlayerId { seat: 1 })
        .unwrap();
    settle(&mut game);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":1}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    settle(&mut game);
    let life = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
        .as_u64()
        .unwrap();
    game.activate_internal_card("s", form.card().abilities[0].node.id, PlayerId { seat: 1 })
        .unwrap();
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        life + 1
    );
}

#[test]
fn named_leave_history_keeps_each_destination_and_repeated_departures_after_replay() {
    use sve_engine::game::player::observation::{HistoryEvent, ObservedZone};
    for (destination, expected) in [
        ("cemetery", ObservedZone::Cemetery),
        ("hand", ObservedZone::Hand),
        ("banish", ObservedZone::Banish),
    ] {
        let repeated = destination == "hand";
        let threshold = if repeated { 2 } else { 1 };
        let query = json!({"event":"left_field","window":"this_turn","who":"self",
            "sel":{"kind":"follower","name":"unit-follower"},"at_least":threshold});
        let program = consumer(&query, false, destination);
        let mut game = initial_game(&program, &card(vec![]), 1);
        let departure = program.card().abilities[2].node.id;
        depart(&mut game, "u0", departure);
        let history = game.observe(Viewer::P1).unwrap().history;
        assert_eq!(
            history
                .subjects
                .iter()
                .filter(|fact| fact.event == HistoryEvent::Leave)
                .count(),
            1
        );
        assert!(
            history
                .subjects
                .iter()
                .filter(|fact| fact.event == HistoryEvent::Leave)
                .all(|fact| fact.destination == Some(expected))
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"][destination]
                .as_array()
                .unwrap()
                .len(),
            1 + usize::from(repeated)
        );
        assert_eq!(
            cost_observation::preview(&game, Viewer::P1).cost.total_pp,
            Some(i64::from(repeated))
        );
        if repeated {
            game.play_internal_card("u0", PlayerId { seat: 1 }).unwrap();
            settle(&mut game);
            depart(&mut game, "u0", departure);
            let history = game.observe(Viewer::P1).unwrap().history;
            assert_eq!(
                history
                    .subjects
                    .iter()
                    .filter(|fact| fact.event == HistoryEvent::Leave)
                    .count(),
                2
            );
            assert!(
                history
                    .subjects
                    .iter()
                    .filter(|fact| fact.event == HistoryEvent::Leave)
                    .all(|fact| fact.destination == Some(ObservedZone::Hand))
            );
            assert_eq!(
                cost_observation::preview(&game, Viewer::P1).cost.total_pp,
                Some(0)
            );
        }
    }
}
