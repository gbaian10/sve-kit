use super::*;

fn named_game(initial: &Value, name: &str) -> Game {
    let (mut snapshot, keywords, mut input) = inputs().clone();
    let mut face = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|card| card["number"] == "BP01-037")
        .unwrap()["faces"][0]
        .clone();
    face["name"] = json!(name);
    snapshot.push_str(&json!({"number":"companion","faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!(["companion.yaml", "format: 1\nkind: effect_set\ncards:\n  companion:\n    status: complete\n    abilities: []\n"]));
    let mut game = game_from_selection(initial, &snapshot, &keywords, &input);
    game.load_internal_card(
        "companion",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

#[test]
fn bp04_032_exact_field_name_controls_cost_and_end_phase_damage() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for (name, companion_seat, matches) in [
            ("ロードジェネラル・ロミオ", seat, true),
            ("ロードジェネラル・ロミオの影", seat, false),
            ("ロードジェネラル・ロミオ", opponent, false),
        ] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"juliet","card":"BP04-032"}]);
            initial["players"][companion_seat]["zones"]["field"] =
                json!([{"id":"companion","card":"companion"}]);
            if initial["players"][opponent]["zones"]["field"].is_null() {
                initial["players"][opponent]["zones"]["field"] = json!([]);
            }
            initial["players"][opponent]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"enemy","card":"BP01-037"}));
            let mut game = named_game(&initial, name);
            assert!(matches!(
                game.play_internal_card(
                    "juliet",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ));
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[seat]["pp"]["current"],
                if matches { 9_u8 } else { 7_u8 }
            );
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = question.domain else {
                panic!("end trigger");
            };
            let play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            if matches {
                let CardPlay::Awaiting(choice) = play else {
                    panic!("named condition selects target");
                };
                let InputDomain::Targets(domain) = &choice.domain else {
                    panic!("target domain");
                };
                let handle = domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("BP01-037"))
                    .unwrap()
                    .handle;
                resolved(
                    game.answer_internal_card(
                        choice.actor,
                        choice.token,
                        Answer::Targets(vec![handle]),
                    )
                    .unwrap(),
                );
            } else {
                resolved(play);
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if matches { 2_u8 } else { 0_u8 }
            );
        }
    }
}
