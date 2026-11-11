use super::super::{Result, invalid, tokens};
use crate::ir::walk::{self, NodeRef, Traversal, Visitor};
use crate::ir::{
    Ability, AbilityId, Action, CardProgram, Instruction, InstructionId, Located, Program,
    SourceSpan, WellFormedCardProgram, WellFormedProgram,
};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::ops::Deref;

#[derive(Clone, Debug)]
pub(crate) struct AdmittedCard {
    checked: Arc<WellFormedCardProgram>,
    abilities: Vec<PlannedAbility>,
}

impl AdmittedCard {
    pub(super) fn compile(checked: &WellFormedCardProgram) -> Result<Self> {
        let abilities = checked
            .card()
            .abilities
            .iter()
            .map(PlannedAbility::compile)
            .collect::<Result<_>>()?;
        Ok(Self {
            checked: Arc::new(checked.clone()),
            abilities,
        })
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
            .find(|ability| ability.definition == *definition)
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

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct PlannedAbility {
    definition: Located<Ability>,
    plan: Arc<BodyPlan>,
}
impl Deref for PlannedAbility {
    type Target = Located<Ability>;
    fn deref(&self) -> &Self::Target {
        &self.definition
    }
}
impl PlannedAbility {
    pub(super) fn compile(definition: &Located<Ability>) -> Result<Self> {
        let mut compiler = Compiler::new(&definition.source);
        compiler.root = true;
        walk::ability(definition, &mut compiler);
        let plan = compiler.finish(definition.node.body.clone().unwrap_or_else(|| Program {
            scope: definition.node.scope,
            steps: vec![],
        }))?;
        Ok(Self {
            definition: definition.clone(),
            plan: Arc::new(plan),
        })
    }
    pub(in crate::game::internal) fn into_parts(self) -> (Ability, Arc<BodyPlan>) {
        (self.definition.node, self.plan)
    }
    pub(in crate::game::internal) const fn located(&self) -> &Located<Ability> {
        &self.definition
    }
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct BodyPlan {
    program: Program,
    tokens: BTreeMap<InstructionId, tokens::TokenPlan>,
    grants: Vec<(Option<Instruction>, PlannedAbility)>,
    delays: BTreeMap<InstructionId, (Instruction, Arc<Self>)>,
}
impl BodyPlan {
    fn compile(program: &Program, source: &SourceSpan) -> Result<Self> {
        let mut compiler = Compiler::new(source);
        walk::program(program, "", &mut compiler);
        compiler.finish(program.clone())
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
                owner_matches && ability.definition.node == definition.node
            })
            .map(|(_, ability)| Self::relocate(ability, &definition.source))
            .ok_or_else(|| invalid("missing or foreign granted definition plan"))
    }
    fn relocate(ability: &PlannedAbility, source: &SourceSpan) -> PlannedAbility {
        let mut result = ability.clone();
        result.definition.source = source.clone();
        result
    }
    pub(in crate::game::internal) fn delayed(
        &self,
        instruction: &Instruction,
        callback: Located<Ability>,
    ) -> Result<PlannedAbility> {
        let (_, plan) = self
            .delays
            .get(&instruction.id)
            .filter(|(original, _)| original == instruction)
            .ok_or_else(|| invalid("missing or foreign delayed plan"))?;
        let Action::Delay {
            trigger, source, ..
        } = &instruction.action
        else {
            return Err(invalid("delayed plan action"));
        };
        if callback.node.body.as_ref() != Some(&plan.program)
            || callback.node.trigger.as_ref() != Some(trigger)
            || callback.node.source != *source
        {
            return Err(invalid("callback differs from admitted delayed definition"));
        }
        Ok(PlannedAbility {
            definition: callback,
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
    fn finish(self, program: Program) -> Result<BodyPlan> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(BodyPlan {
            program,
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
        AbilityOrigin, AbilitySource, Binding, Expr, FaceId, InstructionContext, Nat, Node,
        ObjectOperand, PlayerRef, Posture, ReceiptId, ScopeId, Selector, TokenDestination,
        TokenSpec, TokenTemplate, TokenTemplateId, ZoneKind, ZoneRef,
    };

    #[test]
    fn an_instruction_identity_cannot_authorize_a_different_token_request() {
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
}
