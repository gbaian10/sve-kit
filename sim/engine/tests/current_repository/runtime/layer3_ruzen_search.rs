use super::event_cemetery::finish_checks;
use super::union_burst_batch1::question;
use super::*;
use sve_engine::game::internal::{Fact, InstructionOutcome, SearchAnswer};

#[test]
fn q1331_ruzen_blocks_draws_but_a_synthetic_search_still_adds_a_card_to_hand() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ruzen","card":"BP10-076"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"found","card":"BP01-037"},{"id":"remaining","card":"BP01-037"}
    ]);
    let draw = Action::Draw {
        player: PlayerRef::Controller {},
        count: Expr::Literal(Int(1)),
        mode: StepwiseMode::Exact,
    };
    let program = producer_steps(vec![
        draw.clone(),
        Action::Search {
            decision: DecisionId(2),
            player: PlayerRef::Controller {},
            deck: PlayerRef::Controller {},
            candidate: Binding::new(1, ScopeId(2)),
            filter: Expr::Literal(Bool(true)),
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            groups: None,
            destination: Some(SearchDestination::Single {
                movement: MoveSpec {
                    to: MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
            }),
            continuation: SearchContinuation {
                found: Binding::new(2, ScopeId(3)),
                revealed: Binding::new(3, ScopeId(3)),
            },
        },
        Action::Shuffle {
            deck: PlayerRef::Controller {},
            scope: ShuffleScope::FullZone {},
            randomness: RandomnessPolicy::FreshRequest,
        },
        draw,
    ]);
    let mut board = with_selected_producer_program(&initial, &program, producer_selection());
    let q = question(
        board
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 2 })
            .unwrap(),
    );
    let InputDomain::Search(domain) = &q.domain else {
        panic!("search remains available")
    };
    assert_eq!(q.actor, PlayerId { seat: 2 });
    let result = resolved(
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            )
            .unwrap(),
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Draw(_)))
    );
    for id in [1, 4] {
        assert!(matches!(
            result.progress.receipts[&ReceiptId(id)],
            InstructionOutcome::Cards(CardQuantityOutcome { actual: Nat(0), .. })
        ));
    }
    finish_checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(after["P2"]["deck"].as_array().unwrap().len(), 1);
    assert!(after["game"]["winner"].is_null());
}
