use super::{
    AbilitySource, BodyContext, Located, LoweringError, Node, SourceSpan, ast, unsupported,
};
use crate::ir::{
    AbilityChoiceKind, AbilityCostPolicy, AbilityOperand, AbilityTriggerPolicy, Action, Binding,
    Cardinality, ChoiceDomain, Decision, DecisionId, DecisionKind, Declinable, Instruction,
    InstructionContext, InstructionId, Nat, ObjectOperand, PlayerRef, ReceiptId, Visibility,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Nested ability plays use the existing lexical target and instruction environment."
)]
impl BodyContext<'_> {
    pub(super) fn ability_play(
        &mut self,
        value: &ast::PlayAbility,
        path: &str,
        span: &SourceSpan,
        source: &AbilitySource,
    ) -> Result<Vec<Located<Node>>, LoweringError> {
        let (holder, filter, cost_policy) = match value {
            ast::PlayAbility::Trigger {
                of: ast::Objects::This(_),
                trigger: ast::AbilityPlayTrigger::EvolveTime,
                cost_policy: ast::AbilityPlayCost::Normal,
            } => (
                ObjectOperand::SourceHolder {},
                AbilityChoiceKind::EvolutionTime,
                AbilityCostPolicy::Normal,
            ),
            ast::PlayAbility::Ability {
                of: ast::Objects::Bound(name),
                ability: invocation,
                cost_policy: ast::AbilityPlayCost::WaiveOriginal,
            } if invocation.n == 1 && self.ability_holders.contains(name) => {
                let selection = *self.targets.get(name).ok_or_else(|| unsupported(path))?;
                (
                    ObjectOperand::Selected { selection },
                    AbilityChoiceKind::UnionBurst,
                    AbilityCostPolicy::WaiveOriginal,
                )
            }
            ast::PlayAbility::Trigger { .. } | ast::PlayAbility::Ability { .. } => {
                return Err(unsupported(path));
            }
        };
        let binding = Binding::new(self.allocator.id(path)?, self.scope);
        let decision = Decision {
            id: DecisionId(self.card_allocator.id(path)?),
            captures: vec![],
            actor: PlayerRef::Controller {},
            domain: ChoiceDomain::Abilities {
                holder,
                filter,
                binding,
            },
            kind: DecisionKind::ResolutionChoice {
                may: Declinable::Required,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                precondition: None,
            },
            visibility: Visibility::Public {},
        };
        let id = self.card_allocator.id(path)?;
        let instruction = Instruction {
            id: InstructionId(id),
            receipt: ReceiptId(id),
            actor: PlayerRef::Controller {},
            source: source.clone(),
            precondition: None,
            context: InstructionContext::Effect,
            action: Action::PlayAbility {
                ability: AbilityOperand::Selected { selection: binding },
                controller: PlayerRef::Controller {},
                cost_policy,
                trigger_policy: AbilityTriggerPolicy::IgnoreAsAuthorized,
            },
        };
        Ok(vec![
            Self::located_step(span, path, Node::Decision { decision }),
            Self::located_step(
                span,
                path,
                Node::Instruction {
                    instruction: Box::new(instruction),
                },
            ),
        ])
    }
}
