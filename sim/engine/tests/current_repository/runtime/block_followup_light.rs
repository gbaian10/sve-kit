use super::event_cemetery::{finish_checks, move_named};
use super::union_burst_batch1::{finish, pending};
use super::*;

fn settled(board: &mut Game, play: CardPlay) {
    resolved(finish(board, play));
    finish_checks(board);
}
#[test]
fn remodel_both_printings_filter_trait_and_token_kind_and_create_fresh_eligible_copies() {
    for card in ["BP16-121", "BP16-P28"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"spell1","card":card},{"id":"spell2","card":card}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"chosen","card":"BP05-T04","state":{"power":7_u8,"hp":7_u8,"max_hp":7_u8,"cost":9_u8,"counters":{"mark":4_u8}}},
            {"id":"fairy","card":"BP01-T03"},{"id":"ordinary","card":"BP22-045"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex_artifact","card":"BP05-T04"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"opponent_artifact","card":"BP05-T04"}]);
        let mut board = game(&initial);
        for (spell, candidates) in [("spell1", 1_usize), ("spell2", 2)] {
            let play = board
                .play_internal_card(spell, PlayerId { seat: 1 })
                .unwrap();
            let CardPlay::Awaiting(question) = &play else {
                panic!("printed target choice");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("printed token targets");
            };
            assert_eq!(domain.members.len(), candidates);
            settled(&mut board, play);
        }
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 5);
        for id in view["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|id| !["chosen", "fairy", "ordinary"].contains(id))
        {
            let fresh = &view["objects"][id];
            assert_eq!(fresh["name"], "エンシェントアーティファクト");
            assert_eq!(fresh["power"], 3_u8);
            assert_eq!(fresh["max_hp"], 1_u8);
            assert_eq!(fresh["counters"], json!({}));
        }
        assert_eq!(view["objects"]["chosen"]["power"], 7_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["spell1", "spell2"]));
    }
}
#[test]
fn drazuel_both_evolved_printings_pay_matching_ex_cards_debuff_heal_and_return_after_leaving() {
    for card in ["BP19-058", "BP19-SL16"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"host","card":"BP19-057"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fee1","card":"BP19-057"},{"id":"fee2","card":"BP19-057"},{"id":"wrong_trait","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"small","card":"BP01-037","state":{"power":4_u8,"hp":4_u8,"max_hp":4_u8}},{"id":"large","card":"BP01-037","state":{"power":6_u8,"hp":6_u8,"max_hp":6_u8}}]);
        let action = move_named(
            ZoneKind::Field,
            OwnerZone::Cemetery,
            "禁牙の執行者・ドラズエル",
        );
        let mut board = with_producer(&initial, action);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"host","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        let mut play = pending(&mut board, card);
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::CostMaterials(domain) => {
                    assert_eq!(
                        domain.members.len(),
                        2,
                        "only cards with the printed trait can pay the two-card cost"
                    );
                    Answer::CostMaterials(
                        domain.members.iter().map(|member| member.handle).collect(),
                    )
                }
                InputDomain::Abilities(_)
                | InputDomain::ResourceEntryPayment { .. }
                | InputDomain::WardEntry { .. }
                | InputDomain::Quick { .. }
                | InputDomain::BeforeDraw
                | InputDomain::Pending(_)
                | InputDomain::TriggerQuota(_)
                | InputDomain::Integer(_)
                | InputDomain::Order(_)
                | InputDomain::DamageReplacements(_)
                | InputDomain::DriveTrigger { .. }
                | InputDomain::Search(_)
                | InputDomain::SearchPartition(_)
                | InputDomain::Distribution(_)
                | InputDomain::SearchReplacement
                | InputDomain::Targets(_)
                | InputDomain::TokenCapacity { .. }
                | InputDomain::Modes { .. } => panic!("unexpected evolution payment question"),
            };
            play = board
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        settled(&mut board, play);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["ex"], json!(["wrong_trait"]));
        assert_eq!(after["P1"]["leader"]["life"], 15_u8);
        assert_eq!(after["objects"]["large"]["power"], 1_u8);
        assert_eq!(after["objects"]["large"]["max_hp"], 1_u8);
        assert_eq!(after["P2"]["cemetery"], json!(["small"]));
        activate(&mut board, "P1");
        finish_checks(&mut board);
        let lastword = pending(&mut board, card);
        settled(&mut board, lastword);
        assert!(
            board.projection(View::Referee).unwrap()["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("host"))
        );
    }
}
