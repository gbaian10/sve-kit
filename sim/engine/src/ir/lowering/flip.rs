//! Resolution flips use a public face-up binding or a private face-down choice.

use super::{
    AbilitySource, Action, BodyContext, Located, LoweringError, Node, PlayerRef, SourceSpan, ast,
    unsupported,
};
use crate::ir::public_evolve;
use crate::ir::{
    Binding, Cardinality, Control, DecisionKind, Declinable, Instruction, InstructionContext,
    InstructionId, Nat, Program, ReceiptId, Selector,
};

impl BodyContext<'_> {
    pub(super) fn flip_up(
        &mut self,
        value: &ast::FlipUpSelection,
        path: &str,
        span: &SourceSpan,
        source: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        if value.upto == 0 {
            return Err(unsupported(path));
        }
        let scope = self.card_allocator.scope(path)?;
        let decision_path = format!("{path}/flip_up/sel");
        let binding = Binding::new(self.allocator.id(&decision_path)?, scope);
        let cardinality = Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(value.upto)),
        };
        let decision = super::flip_cost::FlipSelection {
            filter: &value.sel,
            bounds: cardinality,
            before: false,
            kind: DecisionKind::ResolutionChoice {
                may: Declinable::Required,
                cardinality,
                precondition: None,
            },
        }
        .lower(self.card_allocator, binding, &decision_path)?;
        let id = self.card_allocator.id(path)?;
        let flip = Instruction {
            id: InstructionId(id),
            actor: PlayerRef::Controller {},
            source: source.clone(),
            precondition: None,
            action: Action::FlipUp {
                objects: Selector::Bound { reference: binding },
                faces: vec![],
            },
            receipt: ReceiptId(id),
            context: InstructionContext::Effect,
        };
        Ok(Self::located_step(
            span,
            path,
            Node::Control {
                control: Control::Sequence {
                    body: Program {
                        scope,
                        steps: vec![
                            Self::located_step(span, &decision_path, Node::Decision { decision }),
                            Self::located_step(
                                span,
                                path,
                                Node::Instruction {
                                    instruction: Box::new(flip),
                                },
                            ),
                        ],
                    },
                },
            },
        ))
    }

    pub(super) fn flip_down(
        &mut self,
        value: &ast::FlipDown,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let objects = match value {
            ast::FlipDown::Public { r#ref: query, .. } => {
                let objects = self.counted_objects(query, path)?;
                if !public_evolve::selector(&objects) {
                    return Err(unsupported(path));
                }
                objects
            }
            ast::FlipDown::Selected(value) => {
                let (ast::ObjectAction::Short(objects)
                | ast::ObjectAction::Long { r#ref: objects, .. }) = value;
                if !matches!(objects, ast::Objects::Bound(name) if self.flip_targets.contains(name))
                {
                    return Err(unsupported(path));
                }
                self.objects(objects, path)?
            }
        };
        Ok(Action::FlipDown { objects })
    }
}
