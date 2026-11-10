//! Canonical card-and-edge selection lowers to the existing private role partition.
use super::{
    AbilitySource, BodyContext, Located, LoweringError, Node, SourceSpan, ast, unsupported,
};
use crate::ir::{
    Action, Binding, Cardinality, ChoiceDomain, Instruction, InstructionContext, InstructionId,
    MoveDestination, MoveReason, MoveSpec, Nat, OwnerZone, PlayerRef, Position, ReceiptId,
    RoleCardinality, Selector, TargetRole, TargetRoleSpec, hand_position,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The canonical pair shares the ordinary private-hand selector and mover."
)]
impl BodyContext<'_> {
    pub(super) fn hand_position_pair(
        &mut self,
        choice: &ast::Choose,
        movement: &ast::Move,
        choice_path: &str,
        move_path: &str,
        source: &SourceSpan,
        ability: &AbilitySource,
    ) -> Result<Vec<Located<Node>>, LoweringError> {
        let ast::Move {
            r#ref: Some(ast::MoveObjects::Field(ast::Objects::Bound(name))),
            top: None,
            to: ast::Destination::Deck,
            acted: None,
            position: Some(ast::Placement::Chosen { chosen }),
            order: None,
            result: None,
            _by: None,
        } = movement
        else {
            return Err(unsupported(move_path));
        };
        if name != &choice.bind
            || chosen != name
            || choice.n != Some(1)
            || choice.any
            || choice.upto.is_some()
            || choice.may.is_some()
            || choice.groups.is_some()
            || choice.distinct_by.is_some()
            || choice.cost_sum_le.is_some()
            || choice.order.is_some()
            || choice.reveal.is_some()
        {
            return Err(unsupported(choice_path));
        }
        let mut located = self.choose(choice, choice_path, source)?;
        let Node::Decision { decision } = &mut located.node else {
            return Err(unsupported(choice_path));
        };
        let roles = [1_u64, 2]
            .into_iter()
            .map(|role| {
                Ok(TargetRole {
                    role: Nat(role),
                    cardinality: RoleCardinality::Fixed {
                        cardinality: Cardinality {
                            minimum: Nat(0),
                            maximum: Some(Nat(1)),
                        },
                    },
                    binding: Binding::new(self.allocator.id(choice_path)?, self.scope),
                })
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        let ChoiceDomain::Objects { target_roles, .. } = &mut decision.domain else {
            return Err(unsupported(choice_path));
        };
        *target_roles = Some(TargetRoleSpec { roles });
        let position_roles = hand_position::choice(decision)
            .ok_or_else(|| unsupported(choice_path))?
            .roles
            .clone();
        let mut steps = vec![located];
        for (role, position) in position_roles
            .iter()
            .zip([Position::Top {}, Position::Bottom {}])
        {
            let id = self.card_allocator.id(move_path)?;
            steps.push(Self::located_step(
                source,
                move_path,
                Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(id),
                        receipt: ReceiptId(id),
                        actor: PlayerRef::Controller {},
                        source: ability.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        action: Action::Move {
                            objects: Selector::Bound {
                                reference: role.binding,
                            },
                            movement: MoveSpec {
                                to: MoveDestination::Owners {
                                    zone: OwnerZone::Deck,
                                },
                                initial_posture: None,
                                entry_effects: vec![],
                                position: Some(position),
                                order: None,
                            },
                            reason: MoveReason::Put,
                        },
                    }),
                },
            ));
        }
        Ok(steps)
    }
}
