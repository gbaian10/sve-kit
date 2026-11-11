use super::*;
use crate::ir::walk::{self, NodeRef, Traversal, Visitor};
use alloc::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Inventory {
    open: Vec<String>,
    scopes: Vec<ScopeId>,
    seen_scopes: BTreeSet<ScopeId>,
    sources: Vec<SourceSpan>,
    instructions: BTreeMap<InstructionId, (Vec<ScopeId>, Option<String>, String)>,
    boundaries: Vec<&'static str>,
    instruction_boundaries: BTreeMap<InstructionId, Vec<&'static str>>,
    skip_costs: bool,
}

impl Visitor for Inventory {
    fn enter(&mut self, node: NodeRef<'_>, path: &str) -> Traversal {
        self.open.push(path.to_owned());
        match node {
            NodeRef::Source(source) => self.sources.push(source.clone()),
            NodeRef::Program(program) => {
                self.scopes.push(program.scope);
                self.seen_scopes.insert(program.scope);
            }
            NodeRef::Control(Control::Repeat { scope, .. }) => {
                self.seen_scopes.insert(*scope);
            }
            NodeRef::Ability(_) => self.boundaries.push("ability"),
            NodeRef::Action(Action::Delay { .. }) => self.boundaries.push("delay"),
            NodeRef::Instruction(instruction) => {
                self.instruction_boundaries
                    .insert(instruction.id, self.boundaries.clone());
                self.instructions.insert(
                    instruction.id,
                    (
                        self.scopes.clone(),
                        self.sources.last().map(|source| source.card_id.clone()),
                        path.to_owned(),
                    ),
                );
            }
            NodeRef::CostSpec(_) if self.skip_costs => return Traversal::Skip,
            NodeRef::Control(_)
            | NodeRef::Action(_)
            | NodeRef::Effect(_)
            | NodeRef::CostSpec(_)
            | NodeRef::ObjectOperand(_) => {}
        }
        Traversal::Descend
    }

    fn leave(&mut self, node: NodeRef<'_>, path: &str) {
        assert_eq!(self.open.pop().as_deref(), Some(path));
        match node {
            NodeRef::Source(source) => assert_eq!(self.sources.pop().as_ref(), Some(source)),
            NodeRef::Program(program) => assert_eq!(self.scopes.pop(), Some(program.scope)),
            NodeRef::Ability(_) => assert_eq!(self.boundaries.pop(), Some("ability")),
            NodeRef::Action(Action::Delay { .. }) => {
                assert_eq!(self.boundaries.pop(), Some("delay"));
            }
            NodeRef::Control(_)
            | NodeRef::Instruction(_)
            | NodeRef::Action(_)
            | NodeRef::Effect(_)
            | NodeRef::CostSpec(_)
            | NodeRef::ObjectOperand(_) => {}
        }
    }
}

fn leaf(scope: u64) -> Program {
    Program {
        scope: ScopeId(scope),
        steps: vec![instruction(
            Action::Draw {
                player: PlayerRef::Controller {},
                count: Expr::Literal(Int(1)),
                mode: StepwiseMode::Exact,
            },
            scope,
            &format!("source-{scope}"),
        )],
    }
}

fn cost(id: u64) -> CostSpec {
    let Node::Instruction { mut instruction } = instruction(
        Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(1)),
        },
        id,
        "cost",
    )
    .node
    else {
        panic!("instruction helper")
    };
    instruction.context = InstructionContext::Cost;
    CostSpec::Ordered {
        steps: vec![CostStep {
            instruction,
            materials: None,
            keyword: None,
        }],
    }
}

fn choice() -> Decision {
    Decision {
        id: DecisionId(1),
        captures: vec![],
        actor: PlayerRef::Controller {},
        domain: ChoiceDomain::Compound { domains: vec![] },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::MayDecline,
            cardinality: Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(1)),
            },
            precondition: None,
        },
        visibility: Visibility::Public {},
    }
}

fn simultaneous() -> Control {
    let Node::Instruction {
        instruction: member,
    } = instruction(
        Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(1)),
            mode: StepwiseMode::Exact,
        },
        11,
        "batch-member",
    )
    .node
    else {
        panic!("instruction helper")
    };
    Control::Simultaneous {
        batch: InstructionBatch {
            members: vec![Located {
                node: *member,
                source: source("batch-member"),
            }],
            membership: BatchMembership::AtBatchStart,
            choice_stages: vec![],
            commit: BatchCommit::SharedCapacityAtomic,
            event_views: BatchViews::BeforeAndAfter,
        },
        receipt: ReceiptId(1),
    }
}

fn modal() -> Control {
    Control::Mode {
        modes: vec![Mode {
            id: ModeId(1),
            costs: cost(80),
            parameters: vec![],
            body: leaf(8),
        }],
        count: Expr::Literal(Nat(1)),
        chosen: ModeDecision {
            id: DecisionId(2),
            actor: PlayerRef::Controller {},
            domain: ChoiceDomain::Compound { domains: vec![] },
            kind: ModeDecisionKind::ResolutionChoice {
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
            },
            visibility: Visibility::Public {},
        },
        choice_kind: ModeChoiceKind::ResolutionAlternative,
    }
}

fn controls() -> Program {
    let branches = vec![
        simultaneous(),
        Control::Match {
            value: DamageRecipient::Leader {
                player: PlayerRef::Controller {},
            },
            leader: Binding::new(1, ScopeId(2)),
            leader_body: leaf(2),
            follower: Binding::new(2, ScopeId(3)),
            follower_body: leaf(3),
        },
        Control::Sequence { body: leaf(4) },
        Control::If {
            condition: Expr::Literal(Bool(false)),
            then_body: leaf(5),
            else_body: Some(leaf(6)),
        },
        Control::May {
            body: leaf(7),
            choice: choice(),
            receipt: ReceiptId(2),
        },
        modal(),
        Control::Repeat {
            body: RepeatBody::Program { body: leaf(9) },
            termination: Termination::Count {
                count: Expr::Literal(Nat(1)),
            },
            check: IterationCheck::BeforeIteration,
            scope: ScopeId(9),
            receipts: vec![],
        },
        Control::Repeat {
            body: RepeatBody::BodyReference {
                ability: AbilityId(1),
                first_clause: Nat(0),
                last_clause: Nat(1),
            },
            termination: Termination::Count {
                count: Expr::Literal(Nat(1)),
            },
            check: IterationCheck::AfterIteration,
            scope: ScopeId(15),
            receipts: vec![],
        },
        Control::EachPlayer {
            players: controller_set(),
            player: Binding::new(3, ScopeId(10)),
            stages: vec![PlayerStage {
                body: leaf(10),
                mode: StageMode::NonChoiceSimultaneous {},
            }],
            choice_order: PlayerOrder::TurnPlayerFirst,
        },
        Control::EachObject {
            input: Selector::Objects { objects: vec![] },
            binding: Binding::new(4, ScopeId(12)),
            body: leaf(12),
            order: None,
        },
        Control::Pay {
            at: PaymentTime::ResolutionChoice,
            costs: cost(130),
            optional: PaymentOptionality::Optional,
            on_paid: leaf(13),
            receipt: ReceiptId(3),
        },
    ];
    program(
        branches
            .into_iter()
            .map(|control| Located {
                node: Node::Control { control },
                source: source("control"),
            })
            .collect(),
    )
}

#[test]
fn observes_every_control_scope_and_payment_without_executing_branches() {
    let input = controls();
    let before = input.clone();
    let mut inventory = Inventory::default();
    walk::program(&input, "/body", &mut inventory);
    assert_eq!(input, before);
    assert_eq!(inventory.open, Vec::<String>::new());
    assert_eq!(inventory.scopes, Vec::<ScopeId>::new());
    assert_eq!(inventory.sources, Vec::<SourceSpan>::new());
    assert_eq!(
        inventory.seen_scopes,
        (1..=10).chain([12, 13, 15]).map(ScopeId).collect()
    );
    assert_eq!(
        inventory
            .instructions
            .keys()
            .copied()
            .collect::<BTreeSet<_>>(),
        (2..=13).chain([80, 130]).map(InstructionId).collect()
    );
    assert!(inventory.instructions.get(&InstructionId(13)).is_some_and(
        |(scopes, source, path)| scopes == &[ScopeId(1), ScopeId(13)]
            && source.as_deref() == Some("source-13")
            && path.ends_with("/control/on_paid/steps/0/instruction")
    ));
}

#[test]
fn an_observer_can_skip_cost_inventory_without_skipping_paid_bodies_or_unbalancing_scopes() {
    let mut inventory = Inventory {
        skip_costs: true,
        ..Inventory::default()
    };
    walk::program(&controls(), "", &mut inventory);
    assert!(!inventory.instructions.contains_key(&InstructionId(80)));
    assert!(!inventory.instructions.contains_key(&InstructionId(130)));
    assert!(inventory.instructions.contains_key(&InstructionId(8)));
    assert!(inventory.instructions.contains_key(&InstructionId(13)));
    assert_eq!(inventory.open, Vec::<String>::new());
    assert_eq!(inventory.scopes, Vec::<ScopeId>::new());
    assert_eq!(inventory.sources, Vec::<SourceSpan>::new());
}

fn delayed_and_granted_card() -> Located<CardProgram> {
    let granted = card(leaf(21), "granted").node.abilities.remove(0).node;
    let mut input = card(
        program(vec![
            instruction(
                Action::Delay {
                    trigger: source_trigger(EventPattern::Phase {
                        binding: Binding::new(50, ScopeId(20)),
                        predicate: Expr::Literal(Bool(true)),
                    }),
                    body: leaf(20),
                    captures: vec![],
                    lifetime: DelayedLifetime::Once {},
                    source: ability_source(),
                },
                30,
                "delay",
            ),
            instruction(
                Action::Grant {
                    subjects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    abilities: vec![GrantedAbility::Definition {
                        ability: Box::new(granted),
                    }],
                    duration: turn(),
                    provider: TextProvider {
                        provider: ObjectView::Current {
                            object: Box::new(ObjectOperand::SourceHolder {}),
                        },
                        face: FaceId("base".into()),
                        ability: AbilityId(1),
                    },
                },
                31,
                "grant",
            ),
            leaf(22).steps.remove(0),
        ]),
        "root",
    );
    input.node.abilities[0]
        .node
        .declarations
        .push(Effect::CostModifier {
            kind: CostKind::Play,
            subjects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            change: CostChange::Delta {
                amount: Expr::Literal(Int(-1)),
            },
            duration: turn(),
            usage: None,
        });
    input
}

#[test]
fn delayed_and_granted_scopes_and_sources_do_not_leak_into_the_next_instruction() {
    let input = delayed_and_granted_card();
    let mut inventory = Inventory::default();
    walk::card(&input, &mut inventory);
    for (id, expected) in [
        (20, vec![ScopeId(1), ScopeId(20)]),
        (21, vec![ScopeId(1), ScopeId(21)]),
        (22, vec![ScopeId(1)]),
    ] {
        assert!(
            inventory
                .instructions
                .get(&InstructionId(id))
                .is_some_and(|(scopes, source, _)| scopes == &expected
                    && source.as_deref() == Some(format!("source-{id}").as_str()))
        );
    }
    assert_eq!(inventory.open, Vec::<String>::new());
    assert_eq!(inventory.sources, Vec::<SourceSpan>::new());
    assert_eq!(inventory.boundaries, Vec::<&str>::new());
    for (id, expected) in [
        (20, vec!["ability", "delay"]),
        (21, vec!["ability", "ability"]),
        (22, vec!["ability"]),
    ] {
        assert_eq!(
            inventory.instruction_boundaries.get(&InstructionId(id)),
            Some(&expected)
        );
    }
    let requirements = Requirements::derive(&input);
    let draws = requirements
        .required
        .iter()
        .find(|item| item.capability.code == "action.draw")
        .expect("nested draw requirements");
    assert_eq!(
        draws
            .locations
            .iter()
            .map(|location| location.source.card_id.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["source-20", "source-21", "source-22"])
    );
    let declaration = requirements
        .required
        .iter()
        .find(|item| item.capability.code == "effect.cost-modifier")
        .expect("declaration after body");
    assert!(
        declaration
            .locations
            .iter()
            .all(|location| location.source.card_id == "root")
    );
}
