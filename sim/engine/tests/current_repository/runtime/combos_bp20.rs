use super::admission_fixture::{finish, position, settle};
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn definition(line: u32) -> AbilityId {
    selection().current["BP20-030"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "The real boards retain Fusion, counter carry, invalid Search answers and capacity in one complete authority transaction."
)]
fn bp20_030_fuses_hand_and_ex_then_searches_distinct_names_with_a_total_cost_and_capacity() {
    for allies in [0_i32, 4_i32] {
        let mut initial = position("BP20-030");
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":"BP20-030"},
            {"id":"hand1","card":"BP15-026"},
            {"id":"hand2","card":"BP15-026"},
            {"id":"wrong","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"flag","card":"BP19-P33"}]);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0_i32..allies)
                .map(|index| json!({"id":format!("ally{index}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"one","card":"BP05-029"},
            {"id":"duplicate","card":"BP05-029"},
            {"id":"two","card":"BP08-030"},
            {"id":"prayer","card":"BP20-029"},
            {"id":"expensive","card":"BP20-023"},
            {"id":"wrong_trait","card":"BP01-037"},
            {"id":"wrong_kind","card":"BP15-026"}
        ]);
        initial["players"]["P2"]["zones"]["deck"] = json!([
            {"id":"mill1","card":"BP01-037"},
            {"id":"mill2","card":"BP01-037"},
            {"id":"mill3","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let fusion = game
            .activate_internal_card("source", definition(2), PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(materials) = fusion else {
            panic!("Fusion material choice");
        };
        let InputDomain::CostMaterials(fusion_domain) = &materials.domain else {
            panic!("Fusion materials");
        };
        assert_eq!(
            fusion_domain.members.len(),
            3,
            "only the three Treasure cards are eligible"
        );
        assert!(game.internal_decision(View::P2).is_none());
        let paid = game
            .answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(
                    fusion_domain
                        .members
                        .iter()
                        .map(|member| member.handle)
                        .collect(),
                ),
            )
            .unwrap();
        finish(&mut game, paid);
        settle(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["融合カウンター"],
            1_i32
        );
        let entry = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, entry);
        settle(&mut game);
        let entered = game.projection(View::Referee).unwrap();
        assert_eq!(
            entered["objects"]["source"]["counters"]["融合カウンター"], 1_i32,
            "the paid Fusion counter follows the source into the field"
        );
        assert_eq!(entered["P1"]["pp"]["current"], 2_i32);
        let activation = game
            .activate_internal_card("source", definition(1), PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(search) = activation else {
            panic!("printed Search");
        };
        let InputDomain::Search(candidates) = &search.domain else {
            panic!("Search candidates");
        };
        assert_eq!(
            candidates.cards.len(),
            5,
            "the spell and the other-trait follower are excluded"
        );
        let paid_state = game.projection(View::Referee).unwrap();
        assert!(
            !paid_state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert!(
            paid_state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        let copies = candidates
            .cards
            .iter()
            .filter(|card| card.card == "BP05-029")
            .map(|card| card.handle)
            .collect::<Vec<_>>();
        let handles = |numbers: &[&str]| {
            numbers
                .iter()
                .map(|number| {
                    candidates
                        .cards
                        .iter()
                        .find(|card| card.card == *number)
                        .unwrap()
                        .handle
                })
                .collect::<Vec<_>>()
        };
        let answer = |cards: Vec<u64>| {
            if let Some(groups) = &candidates.groups {
                assert_eq!(groups.len(), 1);
                SearchAnswer::Grouped(vec![cards])
            } else {
                SearchAnswer::Ungrouped(cards)
            }
        };
        for (invalid, expected) in [
            (copies, "search names must differ"),
            (
                handles(&["BP20-023", "BP08-030", "BP20-029"]),
                "search original cost total exceeds its cap",
            ),
        ] {
            let rejected = game
                .answer_internal_card(search.actor, search.token, Answer::Search(answer(invalid)))
                .unwrap_err();
            assert!(
                format!("{rejected:?}").contains(expected),
                "the printed constraint is the reason for rejection: {rejected:?}"
            );
            assert_eq!(game.projection(View::Referee).unwrap(), paid_state);
        }
        let found = game
            .answer_internal_card(
                search.actor,
                search.token,
                Answer::Search(answer(handles(&["BP05-029", "BP08-030", "BP20-029"]))),
            )
            .unwrap();
        let completed = if allies == 4_i32 {
            let CardPlay::Awaiting(capacity) = found else {
                panic!("one free field place");
            };
            let InputDomain::SearchPartition(domain) = &capacity.domain else {
                panic!("Search capacity");
            };
            assert_eq!(
                domain.cards.len(),
                3,
                "capacity is decided after finding the full legal set"
            );
            assert_eq!(
                domain.cardinality,
                Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1))
                }
            );
            game.answer_internal_card(
                capacity.actor,
                capacity.token,
                Answer::SearchPartition(vec![domain.cards[0].handle]),
            )
            .unwrap()
        } else {
            found
        };
        finish(&mut game, completed);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        let entered_count = if allies == 4_i32 { 1_usize } else { 3_usize };
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            usize::try_from(allies).unwrap() + entered_count
        );
        assert_eq!(
            state["P1"]["deck"].as_array().unwrap().len(),
            7_usize - entered_count
        );
        assert_eq!(state["P1"]["pp"]["current"], 2_i32);
        assert_eq!(state["P1"]["hand"], json!(["wrong"]));
    }
}
