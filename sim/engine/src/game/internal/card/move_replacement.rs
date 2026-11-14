use super::{Ability, Game, Located, Result, admission, unsupported};
use crate::ir::move_replacement::projection;
use crate::ir::static_declarations::{self, Family};
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, Action, Control, Effect, Instruction,
    InstructionContext, InstructionId, Node, ObjectOperand, PlayerRef, Program, ReceiptId,
    ZoneKind,
};

pub(super) fn check(located: &Located<Ability>, declarations: &[Effect]) -> Result<()> {
    let ability = &located.node;
    if ability.kind != AbilityKind::Static
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.active_zones != [ZoneKind::Field]
        || ability.qualification != AbilityQualification::Ordinary
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("static movement replacement header"));
    }
    for effect in declarations {
        let shape = projection(effect).ok_or_else(|| unsupported("static movement replacement"))?;
        if shape.source != &ability.source {
            return Err(unsupported("movement replacement source"));
        }
        // The live selector and condition have the same public authority as an ordinary instruction.
        let body = Program {
            scope: ability.scope,
            steps: vec![Located {
                source: located.source.clone(),
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(1),
                        receipt: ReceiptId(1),
                        actor: PlayerRef::Controller {},
                        source: ability.source.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        action: Action::Destroy {
                            objects: shape.objects.clone(),
                        },
                    }),
                },
            }],
        };
        let program = Program {
            scope: ability.scope,
            steps: vec![Located {
                source: located.source.clone(),
                node: Node::Control {
                    control: Control::If {
                        condition: shape.condition.clone(),
                        then_body: body,
                        else_body: None,
                    },
                },
            }],
        };
        admission::check_card(&program, &located.source, &ability.source, None)?;
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "All movement routes query the same live declarations."
)]
impl Game {
    pub(in crate::game) fn internal_banish_replaces_move(
        &self,
        id: &str,
        to: &str,
    ) -> Result<bool> {
        if to != "cemetery" || self.object(id)?.zone != "field" || self.restricted(id, "banish")? {
            return Ok(false);
        }
        for provider in self.field_ids() {
            for located in self.internal_effective_abilities(&provider)? {
                if located.node.active_zones != [ZoneKind::Field]
                    || !static_declarations::contains(&located.node, Family::MoveReplacement)
                {
                    continue;
                }
                let execution = self.provider_execution(&provider, None)?;
                for effect in &located.node.declarations {
                    let Some(shape) = projection(effect) else {
                        continue;
                    };
                    if execution.boolean(self, shape.condition)?.0
                        && execution
                            .objects(self, shape.objects)?
                            .contains(&execution.reference(self, id)?)
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}
