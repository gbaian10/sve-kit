#![expect(
    clippy::panic,
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
        "light-hot2",
    )
    .unwrap()
}

/// Resolves the play, answering Ward entry, pending triggers and the May with `accept`.
fn settle(game: &mut Game, mut step: Option<CardPlay>, accept: bool) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = &step {
            let answer = match &question.domain {
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::Integer(bounds) => {
                    assert_eq!(bounds.maximum, Some(Nat(1)), "a single May");
                    Answer::Integer(Nat(u64::from(accept)))
                }
                InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                InputDomain::Abilities(_)
                | InputDomain::ResourceEntryPayment { .. }
                | InputDomain::Quick { .. }
                | InputDomain::OptionalCost
                | InputDomain::CostMaterials(_)
                | InputDomain::BeforeDraw
                | InputDomain::Pending(_)
                | InputDomain::TriggerQuota(_)
                | InputDomain::Order(_)
                | InputDomain::DamageReplacements(_)
                | InputDomain::DriveTrigger { .. }
                | InputDomain::Search(_)
                | InputDomain::SearchPartition(_)
                | InputDomain::Distribution(_)
                | InputDomain::SearchReplacement
                | InputDomain::TokenCapacity { .. }
                | InputDomain::Modes { .. } => panic!("unexpected question {:?}", question.domain),
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
            panic!("expected a trigger");
        };
        step = Some(
            game.answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
}

#[test]
fn bp15_101_returns_to_ex_and_heals_when_a_merwyn_enters() {
    for printing in ["BP15-101", "BP15-P21", "EBD04-012"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"merwyn","card":"BP15-094"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":printing}]);
        let mut game = rules_game(&initial);
        let step = game.play_internal_card("merwyn", PLAYER).unwrap();
        settle(&mut game, Some(step), true);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["ex"].as_array().unwrap().contains(&json!("source")),
            "{printing}: the accepted May moves it to EX"
        );
        assert_eq!(state["P1"]["leader"]["life"], 21_u8, "the completed move heals");
    }
}

fn banished(count: u8) -> Value {
    json!((0..count)
        .map(|index| json!({"id":format!("gone{index}"),"card":"BP01-037"}))
        .collect::<Vec<_>>())
}

#[test]
fn bp21_051_creates_its_artifact_once_five_cards_are_banished() {
    for printing in ["BP21-051", "BP21-P25"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
        // The banished deck top is the fifth card.
        initial["players"]["P1"]["zones"]["banish"] = banished(4);
        let mut game = rules_game(&initial);
        let step = game.play_internal_card("source", PLAYER).unwrap();
        settle(&mut game, Some(step), true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 5, "{printing}");
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2, "the artifact entered");
    }
}

#[test]
fn bp22_047_creates_the_edge_artifact_with_ten_banished_cards() {
    for form in ["BP22-047", "BP22-P24"] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP22-046"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["banish"] = banished(10);
        let mut game = rules_game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "{form} evolves"
        );
        settle(&mut game, None, true);
        let state = game.projection(View::Referee).unwrap();
        let tokens: Vec<_> = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| *id != "base")
            .map(|id| state["objects"][id.as_str().unwrap()]["card"].clone())
            .collect();
        assert_eq!(tokens, [json!("BP13-T05")], "{form}: ten banished cards upgrade the token");
    }
}
