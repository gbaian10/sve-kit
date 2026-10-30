#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the real evolution scenario."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn position(form: &str, full: bool) -> Value {
    let mut value = draw_position();
    value["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    value["players"]["P1"]["sep"] = json!(1_u8);
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-077"}]);
    value["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    value["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"recovered","card":"BP07-072"},{"id":"other","card":"BP01-037"}]);
    value["players"]["P1"]["zones"]["ex"] = json!([{"id":"old","card":"BP07-072"}]);
    if full {
        for i in 0_u8..4_u8 {
            value["players"]["P1"]["zones"]["ex"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("filler-{i}"),"card":"BP01-037"}));
        }
    }
    value["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP07-072"}]);
    value["players"]["P1"]["zones"]["deck"] = json!([{"id":"unrelated","card":"BP01-037"}]);
    value
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn head(game: &mut Game, line: u64) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real evolution heads")
    };
    let choice = options.iter().find(|option| option.line == line).unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(choice.handle))
        .unwrap()
}

fn super_head(game: &mut Game) {
    let CardPlay::Awaiting(target) = head(game, 2) else {
        panic!("formal cemetery target")
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("cemetery target domain")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the allied Machine card is a legal target"
    );
    assert_eq!(
        domain.members[0].player.seat, 1,
        "the source controller selects its own cemetery"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("BP07-072"),
        "the real Machine material is the eligible card"
    );
    resolved(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    checks(game);
}

fn empty_search(game: &mut Game) {
    let mut play = head(game, 1);
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Search(domain) = &question.domain else {
            panic!("native Manmar search")
        };
        assert!(
            domain.cards.is_empty(),
            "the unrelated deck card cannot match the native Manmar search"
        );
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![])),
            )
            .unwrap();
    }
    resolved(play);
    checks(game);
}

fn play_cost(game: &Game, id: &str) -> u64 {
    let mut game = game.clone();
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    assert!(
        matches!(
            game.play_internal_card(id, PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_) | CardPlay::Resolved(_)
        ),
        "the native Machine follower enters the field"
    );
    let after = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    before.checked_sub(after).unwrap()
}

#[test]
fn bp17_078_printings_recover_a_formal_machine_target_and_discount_only_that_success() {
    for form in ["BP17-078", "PR-459"] {
        for full in [false, true] {
            let mut game = Game::from_selected_effects(
                selection_with_rules().clone(),
                &position(form, full),
                &Value::Null,
                &Value::Null,
                "native-machine-ex-cost",
            )
            .unwrap();
            assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}})).unwrap().outcome,"resolved");
            checks(&mut game);
            super_head(&mut game);
            empty_search(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("recovered")),
                full
            );
            assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("other")),
                "non-Machine material stays in the cemetery"
            );
            assert_eq!(
                play_cost(&game, "old"),
                4,
                "old copies never inherit a named output discount"
            );
            if !full {
                assert_eq!(
                    play_cost(&game, "recovered"),
                    1,
                    "the actual recovered native follower receives the -3 modifier"
                );
            }
        }
    }
}

#[test]
fn bp17_078_ordinary_evolution_never_installs_the_super_evolution_discount() {
    for form in ["BP17-078", "PR-459"] {
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &position(form, false),
            &Value::Null,
            &Value::Null,
            "native-machine-ordinary-evolution",
        )
        .unwrap();
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve", "source":"source", "evolve_card":"form", "pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        checks(&mut game);
        empty_search(&mut game);
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "ordinary evolution has no super-evolution header"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["recovered", "other"]));
        assert_eq!(state["P1"]["ex"], json!(["old"]));
        assert_eq!(play_cost(&game, "old"), 4);
    }
}
