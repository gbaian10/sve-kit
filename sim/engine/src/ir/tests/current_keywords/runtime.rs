//! Real current selections exercise rules; external instruction producers are synthetic.
use super::{current_inputs, snapshot};
use crate::catalog::Catalog;
use crate::game::internal::{Activation, Answer, CardPlay, DecisionView, InputDomain};
use crate::game::{Game, View};
use crate::ir::*;
use serde_json::{Value, json};
use std::{fs::read_to_string, path::Path};

const NUMBER: &str = "BP01-138";

fn opponent(seat: &str) -> &str {
    if seat == "P1" { "P2" } else { "P1" }
}

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: if seat == "P1" { 1 } else { 2 },
    }
}

fn setup(seat: &str) -> Value {
    json!({"history":"explicit", "turn":{"active":seat,"first_player":"P1","phase":"main",
        "elapsed_turns":{"P1":3_u8,"P2":3_u8}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":10_u8,"max":10_u8},"zones":{}},
        "P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":10_u8,"max":10_u8},"zones":{}}
    }})
}

fn field(player: PlayerRelation) -> Selector {
    let seats = if player == PlayerRelation::All { 2 } else { 1 };
    Selector::Zone {
        players: PlayerSet {
            relation: player,
            relative_to: PlayerRef::Controller {},
            text_quantifier: TextQuantifier::All,
            cardinality: Cardinality {
                minimum: Nat(seats),
                maximum: Some(Nat(seats)),
            },
        },
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: Binding::new(50, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn entry(zone: ZoneKind) -> Action {
    let mut objects = field(PlayerRelation::SelfPlayer);
    if let Selector::Zone { zone: from, .. } = &mut objects {
        *from = zone;
    }
    Action::Move {
        objects,
        movement: MoveSpec {
            to: MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                },
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    }
}

fn producer(actions: Vec<Action>, targets: Option<PlayerRelation>) -> WellFormedCardProgram {
    let source = SourceSpan {
        card_id: "producer".into(),
        card_no: "producer".into(),
        owner: "card".into(),
        scope: "producer".into(),
        face: "0".into(),
        field: "text".into(),
        section: None,
        line: 1,
        fragment: None,
        author_path: "/producer".into(),
    };
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        controller: PlayerRef::Controller {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        },
    };
    let cardinality = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    let parameters = targets
        .map(|relation| Decision {
            id: DecisionId(10),
            captures: vec![],
            actor: PlayerRef::Controller {},
            visibility: Visibility::Public {},
            kind: DecisionKind::PlayParameter {
                slot: PlaySlot::Targets,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            },
            domain: ChoiceDomain::Objects {
                binding: Binding::new(10, ScopeId(1)),
                target_roles: None,
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector: field(relation),
                        cardinality,
                    }],
                    cardinality,
                    distinct_by: None,
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds: vec![],
                },
            },
        })
        .into_iter()
        .collect();
    WellFormedCardProgram::check(CardProgram {
        card_play_costs: vec![],
        abilities: vec![Located {
            source: source.clone(),
            node: Ability {
                id: AbilityId(1),
                scope: ScopeId(1),
                source: origin.clone(),
                kind: AbilityKind::Activated,
                qualification: AbilityQualification::Ordinary,
                active_zones: vec![ZoneKind::Field],
                event_captures: vec![],
                declarations: vec![],
                trigger: None,
                play: Some(PlaySpec {
                    limit: None,
                    captures: vec![],
                    parameters,
                    costs: CostSpec::Ordered { steps: vec![] },
                }),
                body: Some(Program {
                    scope: ScopeId(1),
                    steps: actions
                        .into_iter()
                        .zip(1_u64..)
                        .map(|(action, id)| Located {
                            source: source.clone(),
                            node: Node::Instruction {
                                instruction: Box::new(Instruction {
                                    id: InstructionId(id),
                                    receipt: ReceiptId(id),
                                    actor: PlayerRef::Controller {},
                                    source: origin.clone(),
                                    precondition: None,
                                    context: InstructionContext::Effect,
                                    action,
                                }),
                            },
                        })
                        .collect(),
                }),
            },
        }],
    })
    .unwrap()
}

fn game(initial: &Value, number: &str, program: &WellFormedCardProgram) -> Game {
    let mut selection = if number == NUMBER {
        current_inputs()["keywords"].clone()
    } else {
        current_inputs()["keyword_fanfare"][number].clone()
    };
    if number != NUMBER {
        selection["authors"]
            .as_array_mut()
            .unwrap()
            .push(current_inputs()["keywords"]["authors"][0].clone());
    }
    selected_game(initial, selection, &snapshot(), program)
}

fn selected_game(
    initial: &Value,
    mut selection: Value,
    snapshot: &str,
    program: &WellFormedCardProgram,
) -> Game {
    let empty = json!({"status":"complete","abilities":[]});
    selection["documents"] = json!([[
        "synthetic.yaml",
        json!({"format":1_u8,"kind":"effect_set","cards":{
            "producer":empty,"plain":empty,"amulet":empty
        }})
        .to_string()
    ]]);
    let extra = [("producer","フォロワー"),("plain","フォロワー"),("amulet","アミュレット")]
        .map(|(number,kind)|json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,
            "cost":"1","power":if kind == "アミュレット" {"-"} else {"2"},"hp":if kind == "アミュレット" {"-"} else {"3"},
            "text":"","sections":[],"traits":[]}]}).to_string()).join("\n");
    let snapshot = format!("{}\n{extra}", snapshot.trim_end());
    let registry = read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/keywords.yaml"),
    )
    .unwrap();
    let selected = Catalog::from_current_selection(&snapshot, &registry, &selection).unwrap();
    let mut game = Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "keyword-rules",
    )
    .unwrap();
    game.load_internal_card("producer", program).unwrap();
    for number in ["plain", "amulet"] {
        game.load_internal_card(
            number,
            &WellFormedCardProgram::check(CardProgram {
                abilities: vec![],
                card_play_costs: vec![],
            })
            .unwrap(),
        )
        .unwrap();
    }
    game
}

fn views(game: &Game) -> [Value; 3] {
    [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap())
}

fn state(game: &Game, id: &str) -> Value {
    game.projection(View::Referee).unwrap()["objects"][id].clone()
}

fn question(play: CardPlay) -> DecisionView {
    if let CardPlay::Awaiting(view) = play {
        view
    } else {
        panic!("expected question")
    }
}

fn resolved(play: CardPlay) -> Activation {
    if let CardPlay::Resolved(result) = play {
        result
    } else {
        panic!("expected resolution")
    }
}

#[test]
fn ward_end_phase_choice_is_optional_and_precedes_quick() {
    // CR 7.4.3 / 12.8.2; FAQ Q257 and Q260 fix the trigger -> Ward -> Quick order.
    for seat in ["P1", "P2"] {
        for selected in [json!([]), json!(["ward"])] {
            let mut initial = setup(seat);
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"ward","card":NUMBER,"owner":opponent(seat)}]);
            let mut game = game(&initial, NUMBER, &producer(vec![], None));
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            assert!(
                game.internal_quick_choice().unwrap().is_none(),
                "Ward precedes Quick"
            );
            let before = views(&game);
            assert_eq!(
                game.internal_rule_action(&json!({"do":"guard-act","select":["ward","ward"]}))
                    .unwrap()
                    .outcome,
                "cannot-play"
            );
            assert_eq!(views(&game), before);
            let step = game
                .internal_rule_action(&json!({"do":"guard-act","select":selected}))
                .unwrap();
            assert_eq!(step.outcome, "resolved");
            assert_eq!(
                state(&game, "ward")["acted"],
                !selected.as_array().unwrap().is_empty()
            );
            let q = game.internal_quick_choice().unwrap().unwrap();
            assert_eq!(q.actor, actor(opponent(seat)));
            assert!(matches!(
                q.domain,
                InputDomain::Quick {
                    window: QuickWindow::EndPhase,
                    ..
                }
            ));
        }
    }
}

#[test]
fn follow_keyword_keeps_printed_ward_in_ex_outside_the_follow_query() {
    let mut movement = entry(ZoneKind::Field);
    let Action::Move {
        objects,
        movement: destination,
        ..
    } = &mut movement
    else {
        panic!("movement")
    };
    *objects = Selector::Bound {
        reference: Binding::new(10, ScopeId(1)),
    };
    destination.to = MoveDestination::Owners {
        zone: OwnerZone::Ex,
    };
    let mut raw = producer(
        vec![
            movement,
            Action::Life {
                player: PlayerRef::Controller {},
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(1)),
                },
            },
        ],
        Some(PlayerRelation::SelfPlayer),
    )
    .card()
    .clone();
    let body = raw.abilities[0].node.body.as_mut().unwrap();
    let mut recovery = body.steps[1].clone();
    let Node::Instruction { instruction } = &mut recovery.node else {
        panic!("recovery")
    };
    instruction.id = InstructionId(3);
    instruction.receipt = ReceiptId(3);
    instruction.action = Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(2)),
        },
    };
    body.steps.insert(
        1,
        Located {
            source: recovery.source.clone(),
            node: Node::Control {
                control: Control::If {
                    condition: Expr::Compute(Box::new(Predicate::Exists {
                        objects: follow_keyword::selector(
                            MovementReceipt::Move {
                                receipt: ReceiptReference::new(ReceiptId(1)),
                            },
                            Binding::new(51, ScopeId(1)),
                        ),
                    })),
                    then_body: Program {
                        scope: ScopeId(2),
                        steps: vec![recovery],
                    },
                    else_body: None,
                },
            },
        },
    );
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let mut initial = setup("P1");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"producer"},{"id":"recipient","card":NUMBER}]);
    let mut game = game(&initial, NUMBER, &checked);
    let q = question(
        game.activate_internal_card("source", AbilityId(1), actor("P1"))
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("field target")
    };
    let recipient = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(NUMBER))
        .unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![recipient.handle]))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"], json!(["recipient"]));
    assert_eq!(view["P1"]["leader"]["life"], 21_u8);
}
