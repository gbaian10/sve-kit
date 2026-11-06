#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ex-token-entry",
    )
    .unwrap()
}

fn ability(card: &str, index: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities[index]
        .node
        .id
}

/// Picks the named EX cards, declines Ward entry, and resolves entered tokens' own fanfares.
fn finish(game: &mut Game, mut step: Option<CardPlay>, picks: &[&str]) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = &step {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(
                    domain
                        .members
                        .iter()
                        .filter(|member| picks.iter().any(|card| member.card.as_deref() == Some(*card)))
                        .map(|member| member.handle)
                        .take(picks.len())
                        .collect(),
                ),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                other => panic!("unexpected question {other:?}"),
            };
            step = Some(
                game.answer_internal_card(question.actor, question.token, answer)
                    .unwrap(),
            );
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected an entry trigger");
        };
        step = Some(
            game.answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
}

fn field(game: &Game) -> Vec<String> {
    game.projection(View::Referee).unwrap()["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn bp01_078_puts_its_ex_dragon_token_onto_the_field_with_rush() {
    for printing in ["BP01-078", "BP01-SL12"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"dragon","card":"BP01-T11"}]);
        let mut game = rules_game(&initial);
        let step = game
            .activate_internal_card("source", ability(printing, 1), PLAYER)
            .unwrap();
        finish(&mut game, Some(step), &["BP01-T11"]);
        let state = game.projection(View::Referee).unwrap();
        assert!(field(&game).contains(&"dragon".to_owned()), "{printing}");
        assert!(
            state["objects"]["dragon"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進"))
        );
        assert_eq!(state["P1"]["ex"], json!([]));
    }
}

#[test]
fn bp14_094_sends_itself_to_the_cemetery_to_field_one_ex_fox() {
    for printing in ["BP14-094", "PR-375"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":printing},
            {"id":"fanatic","card":"BP01-144"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"fox","card":"BP14-T06"},
            {"id":"fox_b","card":"BP14-T06"}
        ]);
        let mut game = rules_game(&initial);
        let step = game
            .activate_internal_card("source", ability(printing, 1), PLAYER)
            .unwrap();
        finish(&mut game, Some(step), &["BP14-T06"]);
        let state = game.projection(View::Referee).unwrap();
        let entered = field(&game)
            .iter()
            .filter(|id| id.starts_with("fox"))
            .count();
        assert_eq!(entered, 1, "{printing} puts one fox onto the field");
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
        assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("source")));
    }
}

#[test]
fn bp13_010_evolution_fields_up_to_two_ex_fairy_tokens() {
    for (base, form) in [("BP13-009", "BP13-010"), ("BP13-009", "BP13-P03")] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"fairy","card":"BP01-T03"},
            {"id":"fairy_b","card":"BP01-T03"},
            {"id":"other","card":"BP01-T11"}
        ]);
        let mut game = rules_game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "{form} evolves from the admitted base"
        );
        finish(&mut game, None, &["BP01-T03", "BP01-T03"]);
        let on_field = field(&game);
        assert!(on_field.contains(&"fairy".to_owned()) && on_field.contains(&"fairy_b".to_owned()));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["other"]),
            "the non-fairy token stays in EX"
        );
    }
}
