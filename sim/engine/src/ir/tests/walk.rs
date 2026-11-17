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
