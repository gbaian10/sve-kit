#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenario."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

/// BP21-040 in hand, one Stack ready on the field; the deck holds one card of each search branch.
fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"kerid","card":"BP21-040"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"plain","card":"BP01-037"},
        {"id":"rite","card":"BP09-048"},
        {"id":"sigil","card":"BP01-075"}
    ]);
    initial
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn take_pending(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending occurrence: {:?}", question.domain);
    };
    assert_eq!(options.len(), 1, "one pending occurrence");
    assert_eq!(options[0].card, card, "the occurrence belongs to {card}");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

/// Plays BP21-040 and answers its optional Earth Rite; paying searches the follower branch.
fn play(game: &mut Game, pay: bool) {
    let entry = game
        .play_internal_card("kerid", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "BP21-040 enters before its Fanfare"
    );
    settle(game);
    let mut outcome = take_pending(game, "BP21-040");
    while let CardPlay::Awaiting(question) = outcome {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(pay),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(domain.members.len(), 1, "only the Stack amulet pays");
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => {
                let mut found: Vec<&str> =
                    domain.cards.iter().map(|card| card.card.as_str()).collect();
                found.sort_unstable();
                assert_eq!(
                    found,
                    ["BP01-075", "BP09-048"],
                    "an Earth Rite follower or an Earth Sigil amulet, not the plain follower"
                );
                let handle = domain
                    .cards
                    .iter()
                    .find(|card| card.card == "BP09-048")
                    .unwrap()
                    .handle;
                Answer::Search(SearchAnswer::Ungrouped(vec![handle]))
            }
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        outcome = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    settle(game);
}

/// Moves to the own End phase and resolves BP21-040 against the opposing leader.
fn end_phase(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(game);
    while game.internal_pending_choice().unwrap().is_none() {
        let Some(window) = game.internal_quick_choice().unwrap() else {
            break;
        };
        game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
            .unwrap();
        settle(game);
    }
    let CardPlay::Awaiting(question) = take_pending(game, "BP21-040") else {
        panic!("the End-phase target is chosen before the condition");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("leader-or-follower target: {:?}", question.domain);
    };
    assert_eq!(domain.members.len(), 1, "only the opposing leader is legal");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    settle(game);
}

#[test]
fn bp21_040_searches_after_earth_rite_and_deals_end_damage_only_after_stack_removed() {
    for pay in [true, false] {
        let mut game = game(&position());
        play(&mut game, pay);
        let state = game.projection(View::Referee).unwrap();
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.contains(&json!("rite")), pay, "only paying searches");
        assert_eq!(
            state["P1"]["deck"].as_array().unwrap().len(),
            if pay { 2 } else { 3 }
        );
        end_phase(&mut game);
        // Q2725/Q2726: any own Earth Rite this turn that removed a Stack satisfies the condition.
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if pay { 18_u8 } else { 20_u8 },
        );
    }
}
