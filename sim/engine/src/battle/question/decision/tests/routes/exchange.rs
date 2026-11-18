use super::super::select::project_game;
use super::*;
use crate::battle::tests::{board_fixture, board_fixtures, payment_fixtures};
use crate::game::Game;
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{CardProgram, WellFormedCardProgram, payment_expanded_tree};
use alloc::sync::Arc;
use serde_json::Value;
use sve_battle_contract::Nat as WireNat;
use sve_battle_contract::observation as cost;

fn program() -> WellFormedCardProgram {
    let mut tree = payment_fixtures::x_document(
        payment_expanded_tree(),
        "activate",
        json!({"cost":{"pp":0_i32},"do":[]}),
    )
    .unwrap();
    let entry = &mut tree["cards"]["unit-follower"]["abilities"][0];
    entry.as_object_mut().unwrap().remove("activate");
    entry["play"] = json!({"optional_cost":{"one_of":[
        {"to_cemetery":{"sel":{"who":"self","zone":"field","class":"ニュートラル","acted":false},"n":1_i32}},
        {"banish":{"sel":{"who":"self","zone":"ex","class":"ニュートラル"},"n":1_i32}}
    ]},"optional_cost_delta":-1_i32});
    ExpandedLowerer::new()
        .unwrap()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

pub(super) fn board(actor: Viewer, field: bool, secret: &str) -> Game {
    let program = program();
    let (own, other) = seats(actor);
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"] = json!({"hand":[{"id":"source","card":"unit-follower"}],
        "field":if field {json!([{"id":"field-fee","card":"unit-follower"}])}else{json!([])},
        "ex":[{"id":"ex-fee","card":"unit-amulet"}]});
    initial["players"][other]["zones"] = json!({"hand":[{"id":"hidden","card":secret}]});
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "exchange-routes",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game.play_internal_card("source", actor.player()).unwrap();
    game.advance_player_input();
    let acceptance = question(&game, actor).unwrap();
    assert_eq!(
        game.submit_player(
            actor,
            player::Command::Answer {
                token: acceptance.token,
                answer: player::Answer::OptionalCost(true)
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    game
}

#[test]
fn affordable_exchange_indices_keep_gaps_and_real_before_payment_costs() {
    for actor in [Viewer::P1, Viewer::P2] {
        for (field, route) in [(true, 0_u64), (true, 1), (false, 1)] {
            let mut game = board(actor, field, "unit-spell");
            let native = question(&game, actor).unwrap();
            let (usage, shape, binding) =
                project_game(&game, actor, &QuestionId::new("exchange".into())).unwrap();
            assert_eq!(usage, wire::QuestionUse::ExchangeRoute);
            let wire::QuestionShape::Choose { options, .. } = shape else {
                panic!("exchange routes")
            };
            assert_eq!(options.len(), if field { 2 } else { 1 });
            let option=options.iter().find(|option|matches!(option.value,wire::ChoiceValue::Route {route:index} if index.get()==route)).unwrap();
            let quote = option.cost.as_ref().unwrap();
            assert_eq!(quote.basis, cost::QuoteBasis::BeforePayment);
            assert!(quote.total_pp.is_none());
            assert!(quote.pending.contains(&cost::PendingCost::Materials));
            assert!(!quote.pending.contains(&cost::PendingCost::Exchange));
            assert!(quote.groups.iter().flat_map(|group|&group.lines).any(|line|matches!(line,cost::CostLine::Pp {quote:cost::PpQuote::Known {amount}} if amount.get()==0)));
            let fee_line = if route == 0 {
                cost::CostLine::Move {
                    cards: Some(WireNat::from(1_u32)),
                }
            } else {
                cost::CostLine::Banish {
                    cards: Some(WireNat::from(1_u32)),
                }
            };
            assert!(
                quote
                    .groups
                    .iter()
                    .any(|group| group.lines.contains(&fee_line))
            );
            let answer = Answer::Choose {
                chosen: vec![option.id.clone()],
            };
            assert_eq!(
                game.submit_player(
                    actor,
                    binding
                        .answer(native.actor, &binding.question().id, &answer)
                        .unwrap()
                )
                .kind,
                SubmissionKind::Applied
            );
            let material = question(&game, actor).unwrap();
            let player::QuestionShape::Decision(DecisionShape::CostMaterials(materials)) =
                &material.shape
            else {
                panic!("route material")
            };
            assert_eq!(
                game.submit_player(
                    actor,
                    player::Command::Answer {
                        token: material.token,
                        answer: player::Answer::CostMaterials(
                            materials
                                .members
                                .iter()
                                .map(|member| member.handle)
                                .collect()
                        )
                    }
                )
                .kind,
                SubmissionKind::Applied
            );
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state[seats(actor).0][if route == 0 { "cemetery" } else { "banish" }]
                    .as_array()
                    .unwrap()
                    .contains(&json!(if route == 0 { "field-fee" } else { "ex-fee" }))
            );
            assert_eq!(state[seats(actor).0]["pp"]["current"], 2_i32);
        }
    }
}
