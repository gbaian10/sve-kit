use super::*;
use crate::catalog::Catalog;
use crate::game::internal::{Context, RuntimeAbilitySource};
use crate::ir::*;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use serde_json::{Value, json};

fn fixture() -> Game {
    let snapshot = json!({"number":"unit","faces":[{"name":"unit","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string();
    let registry = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{"unit":{"status":"complete","review":"synthetic","abilities":[]}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let initial = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":2_u8,"max":2_u8},"ep":0_u8,"sep":0_u8,"zones":{}},
        "P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":2_u8,"max":2_u8},"ep":0_u8,"sep":0_u8,"zones":{"field":[{"id":"s","card":"unit"}],"deck":[{"id":"d0","card":"unit"},{"id":"d1","card":"unit"}]}}
    }});
    Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "stage-boundaries",
    )
    .unwrap()
}

fn execution(game: &mut Game) -> Execution {
    Execution {
        plan: None,
        terminated: false,
        context: Context {
            source: "s".into(),
            generation: Generation(0),
            controller: PlayerId { seat: 2 },
        },
        id: ExecutionId(1),
        timestamp: 1,
        ids: game.internal_identities().unwrap(),
        todo: VecDeque::new(),
        slots: BTreeMap::new(),
        receipts: BTreeMap::new(),
        cost_materials: BTreeMap::new(),
        hand_cost_groups: BTreeMap::new(),
        movement_capacity: BTreeMap::new(),
        material_selections: BTreeMap::new(),
        effect_evolution_destinations: BTreeMap::new(),
        facts: super::super::fact_log::Facts::default(),
        fusion_material_pending: vec![],
        waiting: None,
        destroy_guards: Vec::new(),
        look_viewers: BTreeMap::new(),
        player_receipt_links: BTreeMap::new(),
        reveals: Vec::new(),
        allocations: BTreeMap::new(),
        origin: None,
        ability: Some(RuntimeAbilitySource {
            holder: ObjectRef {
                id: ObjectId(1),
                generation: Generation(0),
            },
            instance: AbilityInstanceId(1),
            controller: PlayerId { seat: 2 },
            definition: AbilityId(1),
        }),
        source_information: None,
        trigger_occurrence: None,
        event_reads: super::super::event_reads::EventReads::default(),
        targets: BTreeMap::new(),
        play: None,
        spell: false,
        field_entry: false,
        play_origin: None,
        card_payment: None,
        play_counts: BTreeMap::new(),
        ward_entries: Vec::new(),
        creating: None,
        nested: None,
        cost_override: None,
        play_origin_player: None,
    }
}

#[test]
fn player_instruction_boundary_sets_the_source_and_restores_it_on_success_and_error() {
    let mut game = fixture();
    let mut execution = execution(&mut game);
    let original_epoch = game.state.ir_epoch;
    for fail in [false, true] {
        let result = game.internal_instruction_boundary(&mut execution, |board, state| {
            assert_eq!(board.card_runtime.active_source, state.ability);
            if fail {
                Err(invalid("boundary test"))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.is_err(), fail);
        assert_eq!(game.card_runtime.active_source, None);
    }
    assert_eq!(game.state.ir_epoch, original_epoch + 2);
}

#[test]
fn player_stages_advance_each_draw_instruction_snapshot() {
    let mut game = fixture();
    let mut execution = execution(&mut game);
    let original_epoch = game.state.ir_epoch;
    let player = Binding::new(1, ScopeId(1));
    let span = SourceSpan {
        card_id: "unit".into(),
        card_no: "unit".into(),
        face: "0".into(),
        owner: "card".into(),
        field: "text".into(),
        line: 1,
        fragment: None,
        section: None,
        scope: "fragment".into(),
        author_path: "synthetic".into(),
    };
    let source = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        },
        controller: PlayerRef::Controller {},
    };
    let steps = (1..=2)
        .map(|id| Located {
            source: span.clone(),
            node: Node::Instruction {
                instruction: Box::new(Instruction {
                    id: InstructionId(id),
                    receipt: ReceiptId(id),
                    actor: PlayerRef::Bound { binding: player },
                    source: source.clone(),
                    context: InstructionContext::Effect,
                    precondition: None,
                    action: Action::Draw {
                        player: PlayerRef::Bound { binding: player },
                        count: Expr::Literal(Int(0)),
                        mode: StepwiseMode::Exact,
                    },
                }),
            },
        })
        .collect();
    let control = Control::EachPlayer {
        players: PlayerSet {
            relative_to: PlayerRef::Controller {},
            relation: PlayerRelation::All,
            cardinality: Cardinality {
                minimum: Nat(0),
                maximum: None,
            },
            text_quantifier: TextQuantifier::All,
        },
        player,
        stages: vec![PlayerStage {
            body: Program {
                scope: ScopeId(2),
                steps,
            },
            mode: StageMode::NonChoiceSimultaneous {},
        }],
        choice_order: PlayerOrder::TurnPlayerFirst,
    };
    game.internal_nonchoice_player_stages(&mut execution, &control)
        .unwrap();
    assert_eq!(game.state.ir_epoch, original_epoch + 2);
    assert_eq!(game.card_runtime.active_source, None);
}

#[test]
fn prohibited_empty_deck_does_not_fail_while_the_other_player_finishes_drawing() {
    let mut game = fixture();
    let mut execution = execution(&mut game);
    game.state.continuous.push(json!({
        "effect":{"op":"restrict","action":"draw"},
        "applies_to":["P1.leader"],
        "context":serde_json::to_value(execution.frame()).unwrap()
    }));
    let instruction = Instruction {
        id: InstructionId(1),
        receipt: ReceiptId(1),
        actor: PlayerRef::Controller {},
        source: AbilitySource {
            holder: ObjectOperand::SourceHolder {},
            origin: AbilityOrigin::Printed {
                face: FaceId("0".into()),
                line: Nat(1),
            },
            controller: PlayerRef::Controller {},
        },
        context: InstructionContext::Effect,
        precondition: None,
        action: Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(3)),
            mode: StepwiseMode::Exact,
        },
    };
    game.internal_player_draws(
        &mut execution,
        &instruction,
        &[(PlayerId { seat: 1 }, 3), (PlayerId { seat: 2 }, 3)],
    )
    .unwrap();
    assert!(!game.state.draws_failed.contains("P1"));
    assert!(game.state.draws_failed.contains("P2"));
    assert_eq!(game.zone_ids("P1", "hand").len(), 0);
    assert_eq!(game.zone_ids("P2", "hand").len(), 2);
    assert_eq!(
        execution.receipts[&ReceiptId(1)],
        InstructionOutcome::Cards(CardQuantityOutcome {
            requested: Int(6),
            actual: Nat(2)
        })
    );
}
