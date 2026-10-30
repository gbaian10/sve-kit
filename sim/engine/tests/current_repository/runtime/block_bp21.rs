use super::event_cemetery::finish_checks;
use super::*;

#[expect(
    clippy::panic,
    reason = "Every real ability is selected through the public pending domain."
)]
fn pending(game: &mut Game) -> CardPlay {
    let occurrence = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &occurrence.domain else {
        panic!("native printed occurrence")
    };
    assert_eq!(options.len(), 1, "only one native ability is pending");
    game.answer_internal_card(
        occurrence.actor,
        occurrence.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[expect(
    clippy::panic,
    reason = "The printed Fanfare only asks whether to discard and which Academy card pays."
)]
fn resolve_fanfare(game: &mut Game, accept: bool) {
    let mut play = pending(game);
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(accept)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            assert_eq!(
                domain.members.len(),
                1,
                "the actual Academy card is the only hand material"
            );
            Answer::CostMaterials(vec![domain.members[0].handle])
        } else {
            panic!("native Fanfare discard cost")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    if accept {
        resolved(play);
    } else {
        assert!(
            matches!(play, CardPlay::Cancelled),
            "declining the required head cost cancels only that occurrence"
        );
    }
    finish_checks(game);
}

fn played(base: &str, cemetery: usize, accept: bool) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":base},{"id":"material","card":base}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..cemetery)
            .map(|i| json!({"id":format!("academy-{i}"),"card":base}))
            .chain([json!({"id":"wrong-trait","card":"BP01-037"})])
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":base}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":base}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":if base == "PR-607" { "PR-608" } else { "BP21-042" }}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "block-bp21",
    )
    .unwrap();
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the complete native base is normally played"
    );
    finish_checks(&mut game);
    resolve_fanfare(&mut game, accept);
    game
}

#[test]
fn bp21_041_printings_really_discard_an_academy_card_to_draw_and_can_decline_the_cost() {
    for base in ["BP21-041", "PR-607"] {
        for accept in [false, true] {
            let game = played(base, 0, accept);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 8_u8);
            assert_eq!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("material")),
                accept
            );
            assert_eq!(
                view["P1"]["deck"].as_array().unwrap().len(),
                if accept { 1_usize } else { 2 }
            );
            assert_eq!(
                view["P1"]["hand"],
                if accept {
                    json!(["top"])
                } else {
                    json!(["material"])
                }
            );
        }
    }
}

#[test]
fn bp21_042_printings_real_evolution_guards_both_replacement_and_leader_damage_at_ten_academy_cards()
 {
    for base in ["BP21-041", "PR-607"] {
        for before_payment in [8_usize, 9] {
            let mut game = played(base, before_payment, true);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            finish_checks(&mut game);
            let CardPlay::Awaiting(question) = pending(&mut game) else {
                panic!("formal enemy follower target")
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("native evolution target domain")
            };
            assert_eq!(domain.members.len(), 1);
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["enemy"]["damage"],
                if before_payment == 9 { 4_u8 } else { 2_u8 }
            );
            assert_eq!(
                view["P2"]["leader"]["life"],
                if before_payment == 9 { 19_u8 } else { 20_u8 },
                "R-0007 guards the leader sentence with the same Academy threshold"
            );
            assert_eq!(view["P1"]["pp"]["current"], 7_u8);
        }
    }
}
