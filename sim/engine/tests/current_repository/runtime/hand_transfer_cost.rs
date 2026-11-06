#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed hand-transfer scenarios."
)]
use super::*;
const PLAYER: PlayerId = PlayerId { seat: 1 };

fn finish(game: &mut Game, mut play: CardPlay, accept: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the one eligible hand card can pay"
                );
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "only the controller sees private materials"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            _ => panic!("unexpected question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}
fn fanfare(game: &mut Game, accept: bool) {
    assert!(
        matches!(
            game.play_internal_card("source", PLAYER).unwrap(),
            CardPlay::Entered(_)
        ),
        "the source enters before its fanfare"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare")
    };
    assert_eq!(options.len(), 1, "only this source has a pending fanfare");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play, accept);
    game.resume_internal_card().unwrap();
}

#[test]
fn bp14_118_all_prints_count_the_paid_ex_material_before_the_buff() {
    for number in ["BP14-118", "BP14-P28"] {
        for (existing, accept) in [(2_u8, true), (1, true), (2, false)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"source","card":number},{"id":"material","card":"BP14-008"}
            ]);
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0..existing)
                    .map(|n| json!({"id":format!("own{n}"),"card":"BP14-008"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P2"]["zones"]["ex"] = json!(
                (0..3_u8)
                    .map(|n| json!({"id":format!("other{n}"),"card":"BP14-008"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
            let mut game = game(&initial);
            fanfare(&mut game, accept);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if accept && existing == 2 { 3_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if accept && existing == 2 { 4_u8 } else { 3_u8 }
            );
            assert_eq!(
                state["P1"]["hand"],
                if accept {
                    json!(["drawn"])
                } else {
                    json!(["material"])
                }
            );
            assert_eq!(
                state["P1"]["ex"].as_array().unwrap().len(),
                usize::from(existing) + usize::from(accept)
            );
        }
    }
}

#[test]
fn bp14_008_all_prints_put_the_paid_hand_card_at_the_hidden_deck_bottom() {
    for number in ["BP14-008", "BP14-P02"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},{"id":"material","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
        let mut game = game(&initial);
        fanfare(&mut game, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P1"]["deck"], json!(["top", "material"]));
        assert!(game.projection(View::P2).unwrap()["objects"]["material"].is_null());
    }
}

#[test]
fn bp17_071_all_prints_act_and_transfer_before_counting_only_own_field_ocean() {
    for number in ["BP17-071", "BP17-P17"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":"BP17-071"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":number},{"id":"ally","card":"BP17-071"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP17-071"}]);
        let mut game = game(&initial);
        let ability = selection().current[number].program.card().abilities[0]
            .node
            .id;
        let play = game
            .activate_internal_card("source", ability, PLAYER)
            .unwrap();
        finish(&mut game, play, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["P1"]["ex"], json!(["material"]));
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    }
}
