use super::{Action, BodyContext, LoweringError, ast, unsupported};
use crate::ir::{
    AbilitySource, Expr, Instruction, InstructionContext, InstructionId, Located, MemberProjection,
    Nat, Node, ObjectReceipt, OwnerZone, PlayerRef, ReceiptId, ReceiptLocation, ReceiptReference,
    Selector, SourceSpan, StepwiseMode, ZoneKind, ZoneRef,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Public observations share the body's existing Look receipts."
)]
impl BodyContext<'_> {
    pub(super) fn public_observation(
        &self,
        name: &str,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let Some(receipt @ ObjectReceipt::Look { receipt: reference }) =
            self.observations.get(name)
        else {
            return Err(unsupported(path));
        };
        if !matches!(reference.location(), ReceiptLocation::Local { id } if self.receipt_reads.published_looks.contains(id))
        {
            return Err(unsupported(path));
        }
        Ok(Selector::ReceiptMembers {
            receipt: receipt.clone(),
            projection: MemberProjection::Before,
        })
    }

    pub(super) fn reveal_top(
        &mut self,
        value: &ast::Reveal,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Vec<Located<Node>>, LoweringError> {
        let top = value.top.as_ref().ok_or_else(|| unsupported(path))?;
        if self.scope != self.resolution_scope
            || value.r#ref.is_some()
            || top.n == 0
            || value.bind.is_empty()
            || !matches!(top.who, ast::DeckHolder::Side(ast::LeaderSide::Controller))
            || self.observations.contains_key(&value.bind)
            || self.targets.contains_key(&value.bind)
            || self.searches.contains(&value.bind)
        {
            return Err(unsupported(path));
        }
        let look = ReceiptId(self.card_allocator.id(path)?);
        self.observations.insert(
            value.bind.clone(),
            ObjectReceipt::Look {
                receipt: ReceiptReference::new(look),
            },
        );
        self.receipt_reads.published_looks.insert(look);
        let objects = self.public_observation(&value.bind, path)?;
        let reveal = ReceiptId(self.card_allocator.id(path)?);
        [
            (
                look,
                Action::Look {
                    viewer: PlayerRef::Controller {},
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Deck,
                    },
                    count: Expr::Literal(Nat(top.n)),
                    mode: StepwiseMode::Exact,
                },
            ),
            (reveal, Action::Reveal { objects }),
        ]
        .into_iter()
        .map(|(receipt, action)| {
            let mut located = source.clone();
            path.clone_into(&mut located.author_path);
            Ok(Located {
                source: located,
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(receipt.0),
                        receipt,
                        actor: PlayerRef::Controller {},
                        source: origin.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        action,
                    }),
                },
            })
        })
        .collect()
    }

    pub(super) fn public_reveal_move(
        &self,
        value: &ast::Move,
        path: &str,
    ) -> Result<Option<Action>, LoweringError> {
        let Some(ast::MoveObjects::Observed(observed)) = &value.r#ref else {
            return Ok(None);
        };
        let ast::Observation::Binding(name) = &observed.r#in else {
            return Ok(None);
        };
        if !self.observations.get(name).is_some_and(|r| matches!(r, ObjectReceipt::Look { receipt } if matches!(receipt.location(), ReceiptLocation::Local { id } if self.receipt_reads.published_looks.contains(id)))) { return Ok(None); }
        if observed.except.is_some()
            || value.top.is_some()
            || value.position.is_some()
            || value.order.is_some()
            || value.acted.is_some()
        {
            return Err(unsupported(path));
        }
        let objects = self.public_observation(name, path)?;
        Ok(Some(match value.to {
            ast::Destination::Hand => super::ordering::owner_put(objects, OwnerZone::Hand),
            ast::Destination::Ex => super::ordering::shared_put(objects, ZoneKind::Ex),
            ast::Destination::Field if value.result.is_some() => {
                super::ordering::shared_put(objects, ZoneKind::Field)
            }
            ast::Destination::Field | ast::Destination::Deck | ast::Destination::Cemetery => {
                return Err(unsupported(path));
            }
        }))
    }
}
