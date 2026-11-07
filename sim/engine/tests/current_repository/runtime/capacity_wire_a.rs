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
        "capacity-wire-a",
    )
    .unwrap()
}

fn evolve(game: &mut Game, form: &str, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "{form} evolves"
    );
}

/// Answers every choice with all offered members, keeping orders as offered.
fn settle(game: &mut Game, mut step: Option<CardPlay>, offered: &mut Vec<Vec<String>>) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = &step {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => {
                    let mut cards: Vec<_> = domain
                        .members
                        .iter()
                        .filter_map(|member| member.card.clone())
                        .collect();
                    cards.sort();
                    offered.push(cards);
                    let limit = domain
                        .cardinality
                        .maximum
                        .map_or(domain.members.len(), |maximum| {
                            usize::try_from(maximum.0).unwrap()
                        });
                    Answer::Targets(
                        domain
                            .members
                            .iter()
                            .take(limit)
                            .map(|member| member.handle)
                            .collect(),
                    )
                }
                InputDomain::Order(members) => {
                    Answer::Order(members.iter().map(|member| member.handle).collect())
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::Abilities(_)
                | InputDomain::ResourceEntryPayment { .. }
                | InputDomain::Quick { .. }
                | InputDomain::OptionalCost
                | InputDomain::CostMaterials(_)
                | InputDomain::BeforeDraw
                | InputDomain::Pending(_)
                | InputDomain::TriggerQuota(_)
                | InputDomain::Integer(_)
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
fn bp08_026_revives_up_to_two_cheap_royal_followers() {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP08-025"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-026"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"two_a","card":"BP01-047"},
        {"id":"two_b","card":"BP01-045"},
        {"id":"three","card":"BP01-048"}
    ]);
    let mut game = rules_game(&initial);
    evolve(&mut game, "BP08-026", 3);
    let mut offered = Vec::new();
    settle(&mut game, None, &mut offered);
    // The revived followers' own Fanfares ask later; the first question is the revival.
    assert_eq!(
        offered[0],
        ["BP01-045", "BP01-047"],
        "only royal followers with original cost 2 or less qualify"
    );
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert!(field.contains(&json!("two_a")) && field.contains(&json!("two_b")));
    assert_eq!(state["P1"]["cemetery"], json!(["three"]));
}

#[test]
fn bp05_036_places_three_cheap_cards_in_ex_and_discounts_them() {
    for form in ["BP05-036", "BP05-SL08", "BP20-R06"] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP05-035"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        let mut deck = vec![
            json!({"id":"cheap_a","card":"BP01-047"}),
            json!({"id":"cheap_b","card":"BP01-045"}),
            json!({"id":"cheap_c","card":"BP01-048"}),
        ];
        deck.extend((0_u8..6).map(|index| json!({"id":format!("big{index}"),"card":"BP01-037"})));
        initial["players"]["P1"]["zones"]["deck"] = json!(deck);
        let mut game = rules_game(&initial);
        evolve(&mut game, form, 2);
        let mut offered = Vec::new();
        settle(&mut game, None, &mut offered);
        assert_eq!(offered.len(), 1, "{form}: one Look choice");
        assert_eq!(offered[0].len(), 3, "only the three cards with original cost 3 or less");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!(["cheap_a", "cheap_b", "cheap_c"]));
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 6);
        // BP01-048 costs 3 and is discounted by 3 this turn.
        let pp = state["P1"]["pp"]["current"].clone();
        let step = game.play_internal_card("cheap_c", PLAYER).unwrap();
        let mut ignored = Vec::new();
        settle(&mut game, Some(step), &mut ignored);
        let played = game.projection(View::Referee).unwrap();
        assert_eq!(played["P1"]["pp"]["current"], pp, "the discounted card costs 0");
    }
}
