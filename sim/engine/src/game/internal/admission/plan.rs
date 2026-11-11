use super::super::{Result, invalid, tokens};
use crate::ir::walk::{self, NodeRef, Traversal, Visitor};
use crate::ir::{
    Ability, AbilityId, Action, CardProgram, Instruction, InstructionId, Located, Program,
    SourceSpan, WellFormedCardProgram, WellFormedProgram,
};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::fmt;
use core::ops::Deref;

#[derive(Clone, Debug)]
pub(crate) struct AdmittedCard {
    checked: Arc<WellFormedCardProgram>,
    abilities: Vec<PlannedAbility>,
}

impl AdmittedCard {
    pub(super) fn compile(checked: &WellFormedCardProgram) -> Result<Self> {
        let checked = Arc::new(checked.clone());
        let abilities = checked
            .card()
            .abilities
            .iter()
            .enumerate()
            .map(|(index, definition)| {
                PlannedAbility::from_definition(
                    Definition::Card {
                        owner: Arc::clone(&checked),
                        index,
                    },
                    definition,
                )
            })
            .collect::<Result<_>>()?;
        Ok(Self { checked, abilities })
    }

    pub(crate) fn card(&self) -> &CardProgram {
        self.checked.card()
    }
    pub(in crate::game::internal) fn granted(
        &self,
        parent: AbilityId,
        definition: &Located<Ability>,
    ) -> Result<PlannedAbility> {
        self.abilities
            .iter()
            .find(|ability| ability.node.id == parent)
            .ok_or_else(|| invalid("missing grant provider plan"))?
            .plan
            .granted(None, definition)
    }
    pub(in crate::game::internal) fn checked(&self) -> &WellFormedCardProgram {
        &self.checked
    }
    pub(in crate::game::internal) fn ability(
        &self,
        definition: &Located<Ability>,
    ) -> Result<PlannedAbility> {
        self.abilities
            .iter()
            .find(|ability| ability.located() == definition)
            .cloned()
            .ok_or_else(|| invalid("ability differs from admitted definition"))
    }
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct AdmittedProgram {
    checked: Arc<WellFormedProgram>,
    plan: Arc<BodyPlan>,
}
impl AdmittedProgram {
    pub(super) fn compile(checked: &WellFormedProgram, source: &SourceSpan) -> Result<Self> {
        Ok(Self {
            checked: Arc::new(checked.clone()),
            plan: Arc::new(BodyPlan::compile(checked.program(), source)?),
        })
    }
    pub(in crate::game::internal) fn program(&self) -> &Program {
        self.checked.program()
    }
    pub(in crate::game::internal) fn plan(&self) -> Arc<BodyPlan> {
        Arc::clone(&self.plan)
    }
}

#[derive(Clone)]
enum Definition {
    Card {
        owner: Arc<WellFormedCardProgram>,
        index: usize,
    },
    Owned(Arc<Located<Ability>>),
}
impl Definition {
    #[expect(
        clippy::indexing_slicing,
        reason = "The private index comes from enumerating this same immutable owner's abilities."
    )]
    fn located(&self) -> &Located<Ability> {
        match self {
            Self::Card { owner, index } => &owner.card().abilities[*index],
            Self::Owned(definition) => definition,
        }
    }
}
impl fmt::Debug for Definition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let definition = self.located();
        f.debug_struct("Definition")
            .field("id", &definition.node.id)
            .field("source", &definition.source)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct PlannedAbility {
    definition: Definition,
    plan: Arc<BodyPlan>,
}
impl Deref for PlannedAbility {
    type Target = Located<Ability>;
    fn deref(&self) -> &Self::Target {
        self.located()
    }
}
impl PlannedAbility {
    pub(super) fn compile(definition: &Located<Ability>) -> Result<Self> {
        Self::from_definition(Definition::Owned(Arc::new(definition.clone())), definition)
    }
    fn from_definition(owner: Definition, definition: &Located<Ability>) -> Result<Self> {
        let mut compiler = Compiler::new(&definition.source);
        compiler.root = true;
        walk::ability(definition, &mut compiler);
        Ok(Self {
            definition: owner,
            plan: Arc::new(compiler.finish()?),
        })
    }
    pub(in crate::game::internal) fn into_parts(self) -> (Ability, Arc<BodyPlan>) {
        (self.located().node.clone(), self.plan)
    }
    pub(in crate::game::internal) fn located(&self) -> &Located<Ability> {
        self.definition.located()
    }
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct BodyPlan {
    tokens: BTreeMap<InstructionId, tokens::TokenPlan>,
    grants: Vec<(Option<Instruction>, PlannedAbility)>,
    delays: BTreeMap<InstructionId, (Instruction, Arc<Self>)>,
}
impl BodyPlan {
    fn compile(program: &Program, source: &SourceSpan) -> Result<Self> {
        let mut compiler = Compiler::new(source);
        walk::program(program, "", &mut compiler);
        compiler.finish()
    }
    pub(in crate::game::internal) fn token(
        &self,
        instruction: &Instruction,
    ) -> Result<&tokens::TokenPlan> {
        self.tokens
            .get(&instruction.id)
            .filter(|plan| plan.instruction() == instruction)
            .ok_or_else(|| invalid("missing or foreign token plan"))
    }
    pub(in crate::game::internal) fn granted(
        &self,
        instruction: Option<&Instruction>,
        definition: &Located<Ability>,
    ) -> Result<PlannedAbility> {
        // Static declarations and installed continuous patches retain their provider's root.
        self.grants
            .iter()
            .find(|(owner, ability)| {
                let owner_matches = instruction.map_or_else(
                    || {
                        owner
                            .as_ref()
                            .is_none_or(|owner| !matches!(owner.action, Action::Grant { .. }))
                    },
                    |instruction| owner.as_ref() == Some(instruction),
                );
                owner_matches && ability.located().node == definition.node
            })
            .map(|(_, ability)| Self::relocate(ability, &definition.source))
            .ok_or_else(|| invalid("missing or foreign granted definition plan"))
    }
    fn relocate(ability: &PlannedAbility, source: &SourceSpan) -> PlannedAbility {
        if &ability.source == source {
            return ability.clone();
        }
        let mut definition = ability.located().clone();
        definition.source = source.clone();
        PlannedAbility {
            definition: Definition::Owned(Arc::new(definition)),
            plan: Arc::clone(&ability.plan),
        }
    }
    pub(in crate::game::internal) fn delayed(
        &self,
        instruction: &Instruction,
        callback: Located<Ability>,
    ) -> Result<PlannedAbility> {
        let (original, plan) = self
            .delays
            .get(&instruction.id)
            .filter(|(original, _)| original == instruction)
            .ok_or_else(|| invalid("missing or foreign delayed plan"))?;
        let Action::Delay {
            body,
            trigger,
            source,
            ..
        } = &original.action
        else {
            return Err(invalid("delayed plan action"));
        };
        if callback.node.body.as_ref() != Some(body)
            || callback.node.trigger.as_ref() != Some(trigger)
            || callback.node.source != *source
        {
            return Err(invalid("callback differs from admitted delayed definition"));
        }
        Ok(PlannedAbility {
            definition: Definition::Owned(Arc::new(callback)),
            plan: Arc::clone(plan),
        })
    }
}

struct Compiler {
    sources: Vec<SourceSpan>,
    root: bool,
    instructions: Vec<Instruction>,
    tokens: BTreeMap<InstructionId, tokens::TokenPlan>,
    grants: Vec<(Option<Instruction>, PlannedAbility)>,
    delays: BTreeMap<InstructionId, (Instruction, Arc<BodyPlan>)>,
    error: Option<crate::EngineFailure>,
}
impl Compiler {
    fn new(source: &SourceSpan) -> Self {
        Self {
            sources: vec![source.clone()],
            root: false,
            instructions: vec![],
            tokens: BTreeMap::new(),
            grants: vec![],
            delays: BTreeMap::new(),
            error: None,
        }
    }
    fn finish(self) -> Result<BodyPlan> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(BodyPlan {
            tokens: self.tokens,
            grants: self.grants,
            delays: self.delays,
        })
    }
    fn observe(&mut self, node: NodeRef<'_>) -> Result<Traversal> {
        match node {
            NodeRef::Source(source) => self.sources.push(source.clone()),
            NodeRef::Ability(ability) => {
                if self.root {
                    self.root = false;
                } else {
                    let source = self
                        .sources
                        .last()
                        .ok_or_else(|| invalid("missing plan source"))?
                        .clone();
                    self.grants.push((
                        self.instructions.last().cloned(),
                        PlannedAbility::compile(&Located {
                            node: ability.clone(),
                            source,
                        })?,
                    ));
                    return Ok(Traversal::Skip);
                }
            }
            NodeRef::Instruction(instruction) => {
                self.instructions.push(instruction.clone());
                if matches!(instruction.action, Action::Token { .. }) {
                    self.tokens
                        .insert(instruction.id, tokens::TokenPlan::compile(instruction)?);
                }
                if let Action::Delay { body, .. } = &instruction.action {
                    let source = self
                        .sources
                        .last()
                        .ok_or_else(|| invalid("missing delayed source"))?;
                    self.delays.insert(
                        instruction.id,
                        (
                            instruction.clone(),
                            Arc::new(BodyPlan::compile(body, source)?),
                        ),
                    );
                    return Ok(Traversal::Skip);
                }
            }
            NodeRef::CostSpec(_) => return Ok(Traversal::Skip),
            _ => {}
        }
        Ok(Traversal::Descend)
    }
}
impl Visitor for Compiler {
    fn enter(&mut self, node: NodeRef<'_>, _path: &str) -> Traversal {
        if self.error.is_some() {
            return Traversal::Skip;
        }
        match self.observe(node) {
            Ok(traversal) => traversal,
            Err(error) => {
                self.error = Some(error);
                Traversal::Skip
            }
        }
    }
    fn leave(&mut self, node: NodeRef<'_>, _path: &str) {
        match node {
            NodeRef::Source(_) => {
                self.sources.pop();
            }
            NodeRef::Instruction(_) => {
                self.instructions.pop();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Binding, DelayedLifetime,
        Expr, FaceId, InstructionContext, Nat, Node, ObjectOperand, PlayerRef, Posture, ReceiptId,
        ScopeId, Selector, TokenDestination, TokenSpec, TokenTemplate, TokenTemplateId,
        TriggerSpec, ZoneKind, ZoneRef,
    };

    fn token_request() -> (SourceSpan, Instruction) {
        let source = SourceSpan {
            card_id: "synthetic".into(),
            card_no: "synthetic".into(),
            scope: "fragment".into(),
            owner: "card".into(),
            face: "0".into(),
            field: "text".into(),
            section: None,
            line: 1,
            fragment: Some("plan".into()),
            author_path: "/body".into(),
        };
        let instruction = Instruction {
            id: InstructionId(1),
            actor: PlayerRef::Controller {},
            source: AbilitySource {
                holder: ObjectOperand::SourceHolder {},
                origin: AbilityOrigin::Printed {
                    face: FaceId("0".into()),
                    line: Nat(1),
                },
                controller: PlayerRef::Controller {},
            },
            precondition: None,
            context: InstructionContext::Effect,
            receipt: ReceiptId(1),
            action: Action::Token {
                tokens: vec![TokenSpec {
                    template: TokenTemplate::Fixed {
                        template: TokenTemplateId("synthetic".into()),
                    },
                    count: Expr::Literal(Nat(1)),
                }],
                destination: TokenDestination::Single {
                    zone: Expr::Literal(ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex,
                    }),
                },
                output: Binding::<Selector>::new(1, ScopeId(1)),
                initial_state: None,
                initial_posture: Posture::Standing,
                entry_effects: vec![],
            },
        };
        (source, instruction)
    }

    #[test]
    fn an_instruction_identity_cannot_authorize_a_different_token_request() {
        let (source, instruction) = token_request();
        let checked = WellFormedProgram::check(Program {
            scope: ScopeId(1),
            steps: vec![Located {
                node: Node::Instruction {
                    instruction: Box::new(instruction.clone()),
                },
                source: source.clone(),
            }],
        })
        .unwrap();
        let admitted = super::super::compile(&checked, &source).unwrap();
        let plan = admitted.plan();
        plan.token(&instruction).unwrap();
        let mut foreign = instruction.clone();
        foreign.id = InstructionId(2);
        plan.token(&foreign).unwrap_err();
        foreign.id = instruction.id;
        if let Action::Token { destination, .. } = &mut foreign.action {
            *destination = TokenDestination::Single {
                zone: Expr::Literal(ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                }),
            };
        }
        plan.token(&foreign).unwrap_err();
    }

    #[test]
    fn delayed_callbacks_require_the_original_instruction_body_trigger_and_source() {
        let (source, mut instruction) = token_request();
        let body = Program {
            scope: ScopeId(2),
            steps: vec![],
        };
        let trigger = TriggerSpec::lastword(Binding::new(1, ScopeId(2)));
        instruction.action = Action::Delay {
            body: body.clone(),
            trigger: trigger.clone(),
            source: instruction.source.clone(),
            captures: vec![],
            lifetime: DelayedLifetime::Once {},
        };
        // This boundary test isolates plan pairing from backend shape admission.
        let plan = BodyPlan::compile(
            &Program {
                scope: ScopeId(1),
                steps: vec![Located {
                    source: source.clone(),
                    node: Node::Instruction {
                        instruction: Box::new(instruction.clone()),
                    },
                }],
            },
            &source,
        )
        .unwrap();
        let callback = Located {
            source,
            node: Ability {
                id: AbilityId(instruction.id.0),
                scope: body.scope,
                source: instruction.source.clone(),
                kind: AbilityKind::Triggered,
                qualification: AbilityQualification::Ordinary,
                active_zones: vec![],
                event_captures: vec![],
                play: None,
                trigger: Some(trigger),
                body: Some(body),
                declarations: vec![],
            },
        };
        plan.delayed(&instruction, callback.clone()).unwrap();
        let mut changed_body = callback.clone();
        changed_body.node.body.as_mut().unwrap().scope = ScopeId(3);
        plan.delayed(&instruction, changed_body).unwrap_err();
        let mut changed_trigger = callback.clone();
        changed_trigger
            .node
            .trigger
            .as_mut()
            .unwrap()
            .active_zones
            .clear();
        plan.delayed(&instruction, changed_trigger).unwrap_err();
        let mut changed_source = callback.clone();
        changed_source.node.source.controller = PlayerRef::Owner {};
        plan.delayed(&instruction, changed_source).unwrap_err();
        let mut foreign = instruction;
        foreign.receipt = ReceiptId(2);
        plan.delayed(&foreign, callback).unwrap_err();
    }
}
