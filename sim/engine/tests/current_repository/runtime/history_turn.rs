use super::*;
use std::sync::LazyLock;

fn history_game(initial: &Value) -> Game {
    static SELECTED: LazyLock<SelectedEffects> = LazyLock::new(|| {
        let (mut snapshot, keywords, mut input) = inputs();
        let mut face = snapshot
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|card| card["number"] == "BP01-037")
            .unwrap()["faces"][0]
            .clone();
        face["name"] = json!("history-companion");
        face["cost"] = json!("0");
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        snapshot.push_str(&json!({"number":"history-companion","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["history-companion.yaml", "format: 1\nkind: effect_set\ncards:\n  history-companion:\n    status: complete\n    abilities: []\n"]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        SELECTED.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "history-current",
    )
    .unwrap();
    game.load_internal_card(
        "history-companion",
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
fn bp12_002_combo_three_counts_committed_cards_and_includes_itself() {
    for seat in ["P1", "P2"] {
        for prior in [1_usize, 2] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            let mut hand = vec![json!({"id":"queen","card":"BP12-002"})];
            for n in 0..prior {
                hand.push(json!({"id":format!("prior{n}"),"card":"history-companion"}));
            }
            initial["players"][seat]["zones"]["hand"] = json!(hand);
            initial["players"][seat]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
            let mut game = history_game(&initial);
            let actor = PlayerId {
                seat: seat_number(seat),
            };
            for n in 0..prior {
                assert!(matches!(
                    game.play_internal_card(&format!("prior{n}"), actor)
                        .unwrap(),
                    CardPlay::Entered(_)
                ));
                game.resume_internal_card().unwrap();
            }
            assert!(matches!(
                game.play_internal_card("queen", actor).unwrap(),
                CardPlay::Entered(_)
            ));
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("Fanfare");
            };
            resolved(
                game.answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[seat]["hand"],
                if prior == 2 {
                    json!(["drawn"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                projected_zone_count(&state[seat]["deck"]),
                u64::from(prior != 2)
            );
        }
    }
}
