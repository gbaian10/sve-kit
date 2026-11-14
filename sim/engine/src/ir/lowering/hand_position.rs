//! Canonical card-and-edge selection lowers to the existing private role partition.
use super::{
    AbilitySource, BodyContext, Located, LoweringError, Node, SourceSpan, ast, unsupported,
};
use crate::ir::{
    Action, Binding, Cardinality, ChoiceDomain, Decision, DecisionId, DecisionKind, Declinable,
    Disjointness, Instruction, InstructionContext, InstructionId, MoveDestination, MoveReason,
    MoveSpec, Nat, OwnerZone, PlayerRef, Position, ReceiptId, RoleCardinality, SelectionConstraint,
    SelectionGroup, Selector, TargetRole, TargetRoleSpec, Visibility, hand_position,
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
            by: None,
            entry_effects: None,
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
            steps.push(self.deck_put(
                source,
                move_path,
                ability,
                PlayerRef::Controller {},
                role.binding,
                position,
            )?);
        }
        Ok(steps)
    }

    /// `move {ref, to: deck, position: {one_of: [top, bottom], by}}`: the deciding player splits
    /// every object between the deck top and bottom and orders each pile, then puts them there.
    pub(super) fn deck_edges(
        &mut self,
        movement: &ast::Move,
        path: &str,
        source: &SourceSpan,
        ability: &AbilitySource,
    ) -> Result<Vec<Located<Node>>, LoweringError> {
        let ast::Move {
            r#ref: Some(ast::MoveObjects::Field(objects)),
            top: None,
            to: ast::Destination::Deck,
            acted: None,
            position: Some(ast::Placement::OneOf { one_of, by }),
            order: None,
            result: None,
            by: performer,
            entry_effects: None,
        } = movement
        else {
            return Err(unsupported(path));
        };
        let edges = one_of
            .iter()
            .map(|placement| match placement {
                ast::Placement::Edge(ast::DeckEdge::Top) => Ok(true),
                ast::Placement::Edge(ast::DeckEdge::Bottom) => Ok(false),
                ast::Placement::Chosen { .. }
                | ast::Placement::Nth { .. }
                | ast::Placement::OneOf { .. } => Err(unsupported(path)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !matches!(edges.as_slice(), [true, false] | [false, true]) {
            return Err(unsupported(path));
        }
        let selector = self.objects(objects, path)?;
        let actor = self.participant(Some(by), path)?;
        // `move.by: self` names the effect controller as the one putting the objects there;
        // without it the chooser also puts them (BP01-008 "それのプレイヤーは…置く").
        let performer = if performer.is_some() {
            PlayerRef::Controller {}
        } else {
            actor.clone()
        };
        let any = Cardinality {
            minimum: Nat(0),
            maximum: None,
        };
        let roles = [1_u64, 2]
            .into_iter()
            .map(|role| {
                Ok(TargetRole {
                    role: Nat(role),
                    cardinality: RoleCardinality::Fixed { cardinality: any },
                    binding: Binding::new(self.allocator.id(path)?, self.scope),
                })
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        let decision = Decision {
            id: DecisionId(self.card_allocator.id(path)?),
            captures: vec![],
            actor,
            domain: ChoiceDomain::Objects {
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector,
                        cardinality: any,
                    }],
                    cardinality: any,
                    distinct_by: None,
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds: vec![],
                },
                binding: Binding::new(self.allocator.id(path)?, self.scope),
                target_roles: Some(TargetRoleSpec {
                    roles: roles.clone(),
                }),
            },
            kind: DecisionKind::ResolutionChoice {
                may: Declinable::Required,
                cardinality: any,
                precondition: None,
            },
            visibility: Visibility::Public {},
        };
        let mut steps = vec![Self::located_step(
            source,
            path,
            Node::Decision { decision },
        )];
        for (role, position) in roles.iter().zip([Position::Top {}, Position::Bottom {}]) {
            steps.push(self.deck_put(
                source,
                path,
                ability,
                performer.clone(),
                role.binding,
                position,
            )?);
        }
        Ok(steps)
    }

    /// The deciding player puts one role's objects into their owners' deck at `position`.
    fn deck_put(
        &mut self,
        source: &SourceSpan,
        path: &str,
        ability: &AbilitySource,
        actor: PlayerRef,
        role: Binding<Selector>,
        position: Position,
    ) -> Result<Located<Node>, LoweringError> {
        let id = self.card_allocator.id(path)?;
        Ok(Self::located_step(
            source,
            path,
            Node::Instruction {
                instruction: Box::new(Instruction {
                    id: InstructionId(id),
                    receipt: ReceiptId(id),
                    actor,
                    source: ability.clone(),
                    precondition: None,
                    context: InstructionContext::Effect,
                    action: Action::Move {
                        objects: Selector::Bound { reference: role },
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
        ))
    }
}
