use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending};
use super::*;

fn finish_combat(game: &mut Game) {
    loop {
        finish_checks(game);
        if game.internal_pending_choice().unwrap().is_some() {
            return;
        }
        let Some(question) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
}

#[test]
fn king_snowman_locks_both_players_low_cost_followers_until_it_leaves_and_then_heals() {
    for active in ["P1", "P2"] {
        let opponent = if active == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(active);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        for player in ["P1", "P2"] {
            initial["players"][player]["zones"]["field"] = json!([
                {"id":format!("{player}-low"),"card":"BP01-042"},
                {"id":format!("{player}-high"),"card":"BP01-037","state":{"hp":20_u8,"acted":player != active}}
            ]);
        }
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(
                json!({"id":"source","card":"BP09-040","state":{"hp":1_u8,"acted":active == "P2"}}),
            );
        let mut game = game(&initial);
        for target in [format!("{opponent}.leader"), format!("{opponent}-high")] {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":format!("{active}-low"),"target":target})
                )
                .unwrap()
                .outcome,
                "cannot-attack"
            );
        }
        let (attacker, defender) = if active == "P1" {
            ("source", "P2-high")
        } else {
            ("P2-high", "source")
        };
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":attacker,"target":defender})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        finish_combat(&mut game);
        let lastword = pending(&mut game, "BP09-040");
        resolved(finish(&mut game, lastword));
        finish_checks(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 13_u8);
        assert_eq!(after["P1"]["cemetery"], json!(["source"]));
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":format!("{active}-low"),"target":format!("{opponent}.leader")})).unwrap().outcome,
            "resolved"
        );
    }
}
