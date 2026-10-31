use super::{
    Action, Binding, BodyContext, Expr, LoweringError, Nat, PlayerRef, ZoneKind, ast, unsupported,
};
use crate::ir::{
    AbilitySource, CardName, Cardinality, Control, LkiReason, MemberProjection, MovementReceipt,
    NatOperation, Node, ObjectOperand, ObjectReceipt, ObjectRef, ObjectView, PlayerOrder,
    PlayerRead, PlayerRelation, PlayerSet, PlayerStage, Posture, Predicate, Role, Selector,
    SourceSpan, StageMode, TextQuantifier, TokenDestination, TokenSpec, TokenTemplateId, ZoneRef,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Token creation shares the body binding allocator."
)]
impl BodyContext<'_> {
    pub(super) fn create(
        &mut self,
        value: &ast::Create,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let kind = match value.to {
            ast::CreateZone::Ex => ZoneKind::Ex,
            ast::CreateZone::Field => ZoneKind::Field,
        };
        if value.result.is_some() && value.bind.is_some() {
            return Err(unsupported(path));
        }
        let output = Binding::new(self.allocator.id(path)?, self.scope);
        if let Some(name) = value.result.as_ref().or(value.bind.as_ref())
            && (self.searches.contains(name)
                || self.observations.contains_key(name)
                || self.targets.insert(name.clone(), output).is_some())
        {
            return Err(unsupported(path));
        }
        if kind == ZoneKind::Ex
            && let Some(name) = value.result.as_ref().or(value.bind.as_ref())
        {
            self.ex_outputs
                .insert(name.clone(), Selector::Bound { reference: output });
        }
        let names = value.token.names();
        if names.is_empty() || names.iter().any(String::is_empty) {
            return Err(unsupported(path));
        }
        let tokens = names
            .iter()
            .map(|name| {
                Ok(TokenSpec {
                    template: TokenTemplateId(name.clone()),
                    name: Expr::Literal(CardName(name.clone())),
                    count: self.token_count(value.n.as_ref(), path)?,
                })
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        Ok(Action::Token {
            tokens,
            destination: TokenDestination::Single {
                zone: Expr::Literal(ZoneRef {
                    player: self.token_player(value.by.as_deref().unwrap_or("self"), path)?,
                    kind,
                }),
            },
            output,
            initial_state: None,
            initial_posture: Posture::Standing,
            entry_effects: vec![],
        })
    }
    fn token_player(&self, name: &str, path: &str) -> Result<PlayerRef, LoweringError> {
        if name == "self" {
            return Ok(PlayerRef::Controller {});
        }
        self.players
            .get(name)
            .copied()
            .map(|binding| PlayerRef::Bound { binding })
            .ok_or_else(|| unsupported(path))
    }

    fn token_count(
        &mut self,
        value: Option<&ast::TokenCount>,
        path: &str,
    ) -> Result<Expr<Nat>, LoweringError> {
        let Some(ast::TokenCount::Receipt { count }) = value else {
            return Ok(Expr::Literal(Nat(match value {
                Some(ast::TokenCount::Fixed(count)) => *count,
                _ => 1,
            })));
        };
        let ast::BeforeOnly::Before = count.r#in.view;
        let ast::EqualOnly::Eq = count.r#where.player_cmp.op;
        if self.players.contains_key(&count.binding)
            || count.r#where.player_cmp.left.player.master_of != count.binding
        {
            return Err(unsupported(path));
        }
        let receipt = self
            .observations
            .get(&count.r#in.objects_of)
            .filter(|receipt| {
                matches!(
                    receipt,
                    ObjectReceipt::Movement {
                        receipt: MovementReceipt::Destroy { .. }
                    }
                )
            })
            .cloned()
            .ok_or_else(|| unsupported(path))?;
        let binding: Binding<ObjectRef> = Binding::new(self.allocator.id(path)?, self.scope);
        let player = self.token_player(&count.r#where.player_cmp.right.player, path)?;
        Ok(Expr::Compute(Box::new(NatOperation::Count {
            objects: Selector::Filter {
                input: Box::new(Selector::ReceiptMembers {
                    receipt,
                    projection: MemberProjection::Before,
                }),
                binding,
                predicate: Box::new(Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Live(Box::new(PlayerRead::Role {
                        object: ObjectView::LastKnown {
                            object: Box::new(ObjectOperand::Bound { binding }),
                            reason: LkiReason::ZoneExit,
                        },
                        role: Role::Controller,
                    })),
                    right: Expr::Literal(player),
                }))),
            },
        })))
    }

    pub(super) fn each_player(
        &mut self,
        each: &ast::EachPlayer,
        path: &str,
        span: &SourceSpan,
        source: &AbilitySource,
    ) -> Result<Node, LoweringError> {
        let [stage] = each.stages.as_slice() else {
            return Err(unsupported(path));
        };
        let [ast::Step::Create(create)] = stage.as_slice() else {
            return Err(unsupported(path));
        };
        if matches!(
            each.binding.as_str(),
            "self" | "opp" | "turn" | "non_turn" | "current" | "item"
        ) || self.players.contains_key(&each.binding)
            || self.targets.contains_key(&each.binding)
            || self.observations.contains_key(&each.binding)
            || self.searches.contains(&each.binding)
            || create.by.as_ref() != Some(&each.binding)
            || create.result.is_some()
            || create.bind.is_some()
        {
            return Err(unsupported(path));
        }
        let parent = self.scope;
        self.scope = self.card_allocator.scope(path)?;
        let player = Binding::new(self.allocator.id(path)?, self.scope);
        self.players.insert(each.binding.clone(), player);
        let body = self.steps(stage, &format!("{path}/each_player/stages/0"), span, source);
        self.players.remove(&each.binding);
        self.scope = parent;
        let mut body = body?;
        for step in &mut body.steps {
            if let Node::Instruction { instruction } = &mut step.node {
                instruction.actor = PlayerRef::Bound { binding: player };
            }
        }
        Ok(Node::Control {
            control: Control::EachPlayer {
                players: PlayerSet {
                    relation: PlayerRelation::All,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: TextQuantifier::All,
                },
                player,
                stages: vec![PlayerStage {
                    body,
                    mode: StageMode::NonChoiceSimultaneous {},
                }],
                choice_order: PlayerOrder::TurnPlayerFirst,
            },
        })
    }
}
