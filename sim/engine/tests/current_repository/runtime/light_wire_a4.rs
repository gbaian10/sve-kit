#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "light-wire-a4",
    )
    .unwrap()
}

/// Plays `source`, answering the printed choices; returns every search candidate list.
fn play(game: &mut Game, mode: u64) -> Vec<Vec<String>> {
    let mut searched = Vec::new();
    let mut step = Some(game.play_internal_card("source", PLAYER).unwrap());
    loop {
        while let Some(CardPlay::Awaiting(question)) = &step {
            let answer = match &question.domain {
                InputDomain::DiceReroll { .. } => panic!("unexpected fixture dice reroll"),
                InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                InputDomain::Search(domain) => {
                    let mut cards: Vec<_> =
                        domain.cards.iter().map(|card| card.card.clone()).collect();
                    cards.sort();
                    searched.push(cards);
                    Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
                }
                InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
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
                | InputDomain::SearchPartition(_)
                | InputDomain::Distribution(_)
                | InputDomain::SearchReplacement
                | InputDomain::TokenCapacity { .. } => {
                    panic!("unexpected question {:?}", question.domain)
                }
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
            return searched;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected a trigger");
        };
        step = Some(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
    }
}

#[test]
fn bp10_046_buffs_its_pawn_and_searches_a_cheap_chess_follower_with_five_chess_cards() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP10-046"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..5)
            .map(|index| json!({"id":format!("chess{index}"),"card":"BP03-039"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"cheap","card":"BP03-051"},
        {"id":"costly","card":"BP03-039"}
    ]);
    let mut game = rules_game(&initial);
    let searched = play(&mut game, 1);
    assert_eq!(
        searched,
        [vec!["BP03-051".to_owned()]],
        "only chess followers with original cost 4 or less"
    );
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert!(
        field.contains(&json!("cheap")),
        "the searched follower enters"
    );
    let pawn = field
        .iter()
        .find(|id| *id != "cheap")
        .and_then(Value::as_str)
        .unwrap();
    // マジカルポーン is 2/1; the spell gives +1 attack and Rush.
    assert_eq!(state["objects"][pawn]["power"], 3_u8);
    assert!(
        state["objects"][pawn]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
}
