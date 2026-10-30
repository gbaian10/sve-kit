use super::event_cemetery::finish_checks;
use super::*;

fn initial(card: &str, materials: &[&str], owner: &str, zone: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    value["players"][owner]["zones"][zone] = json!(
        materials
            .iter()
            .enumerate()
            .map(|(i, material)| json!({"id":format!("material-{i}"),"card":material}))
            .collect::<Vec<_>>()
    );
    value
}

#[test]
fn bp10_016_printings_declare_the_formal_target_before_counting_own_ex_original_cost_kinds() {
    for printing in ["BP10-016", "BP10-P04"] {
        for (materials, owner, zone, damage) in [
            (["BP01-T02", "BP01-T03", "BP02-T05"], "P1", "ex", 3_u8),
            (["BP01-T02", "BP01-T03", "BP01-T03"], "P1", "ex", 0),
            (["BP01-T03", "BP01-T03", "BP01-T03"], "P1", "ex", 0),
            (["BP01-T02", "BP01-T03", "BP02-T05"], "P2", "ex", 0),
            (["BP10-016", "BP02-118", "BP01-037"], "P1", "cemetery", 0),
        ] {
            let mut game = Game::from_selected_effects(
                selection_with_rules().clone(),
                &initial(printing, &materials, owner, zone),
                &Value::Null,
                &Value::Null,
                "block-bp10",
            )
            .unwrap();
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            finish_checks(&mut game);
            let occurrence = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &occurrence.domain else {
                panic!("the real Fanfare is pending")
            };
            assert_eq!(options.len(), 1);
            let CardPlay::Awaiting(target) = game
                .answer_internal_card(
                    occurrence.actor,
                    occurrence.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap()
            else {
                panic!("the formal target must be declared even below three EX cost kinds")
            };
            let InputDomain::Targets(domain) = &target.domain else {
                panic!("printed opposing field follower target")
            };
            assert_eq!(domain.members.len(), 1);
            resolved(
                game.answer_internal_card(
                    target.actor,
                    target.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["enemy"]["damage"], damage);
            assert_eq!(
                after["P1"]["pp"]["current"], 9_u8,
                "normal play pays the printed one PP"
            );
        }
    }
}
