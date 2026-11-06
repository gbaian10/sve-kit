use super::union_burst_batch1::{finish, pending};
use super::*;

fn aliases() -> Vec<String> {
    let selected = selection();
    let identity = &selected.current["BP05-019"].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(number, _)| number.clone())
        .collect()
}

#[test]
fn octrice_evolved_printings_capture_to_own_ex_and_discount_only_at_ten_remaining_cards() {
    for card in aliases() {
        for remaining in [9_u8, 10] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-018"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"drawn","card":"BP01-037"},{"id":"spare","card":"BP01-037"}]);
            let mut cemetery = vec![json!({"id":"borrowed","card":"BP01-065"})];
            cemetery.extend(
                (0..remaining).map(|n| json!({"id":format!("grave{n}"),"card":"BP01-037"})),
            );
            initial["players"]["P2"]["zones"]["cemetery"] = json!(cemetery);
            let mut game = game(&initial);
            let evolution = selection().current["BP05-018"]
                .program
                .card()
                .abilities
                .iter()
                .find(|ability| ability.source.line == 2)
                .unwrap()
                .node
                .id
                .0;
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","definition":evolution,"pay":{"pp":0_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            let triggered = pending(&mut game, &card);
            resolved(finish(&mut game, triggered));
            event_cemetery::finish_checks(&mut game);
            let captured = game.projection(View::Referee).unwrap();
            assert_eq!(captured["P1"]["ex"], json!(["borrowed"]));
            assert_eq!(
                captured["P2"]["cemetery"].as_array().unwrap().len(),
                usize::from(remaining)
            );
            let play = game
                .play_internal_card("borrowed", PlayerId { seat: 1 })
                .unwrap();
            resolved(finish(&mut game, play));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if remaining == 10 { 9_u8 } else { 7 }
            );
            assert_eq!(state["P1"]["hand"], json!(["drawn", "spare"]));
            assert_eq!(
                state["P2"]["cemetery"].as_array().unwrap().last().unwrap(),
                "borrowed"
            );
        }
    }
}

#[test]
fn octrice_evolved_printings_play_the_opponents_spell_for_eight_pp_total() {
    for card in aliases() {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"drawn","card":"BP01-037"},{"id":"spare","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"borrowed","card":"SD03-005"}]);
        let mut game = game(&initial);
        let definition = selection().current[&card]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.source.line == 3)
            .unwrap()
            .node
            .id;
        let play = game
            .activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap();
        let result = resolved(finish(&mut game, play));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 2_u8);
        assert_eq!(state["P2"]["cemetery"], json!(["borrowed"]));
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(
                    fact,
                    Fact::CardPlayed {
                        controller: PlayerId { seat: 1 },
                        ..
                    }
                ))
                .count(),
            1
        );
    }
}
