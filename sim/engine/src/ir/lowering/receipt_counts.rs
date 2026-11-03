use super::{Binding, BodyContext, Bool, Expr, Int, LoweringError, ast, unsupported};
use crate::ir::{
    Cardinality, IntFrozen, IntOperation, MemberProjection, MovementReceipt, Nat, ObjectReceipt,
    ObjectView, Predicate, ReceiptLocation, Selector,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Receipt counts use the body's checked producer environment."
)]
impl BodyContext<'_> {
    pub(super) fn discarded_cost(
        &self,
        name: &str,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let Some(ObjectReceipt::Movement {
            receipt: MovementReceipt::Discard { receipt },
        }) = self.observations.get(name)
        else {
            return Err(unsupported(path));
        };
        let ReceiptLocation::Local { id } = receipt.location() else {
            return Err(unsupported(path));
        };
        if self.receipt_reads.discard_bounds.get(id)
            != Some(&Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            })
        {
            return Err(unsupported(path));
        }
        Ok(Expr::Frozen(Box::new(IntFrozen::DiscardedCost {
            receipt: receipt.clone(),
        })))
    }

    pub(super) fn count_of(&self, name: &str, path: &str) -> Result<Expr<Int>, LoweringError> {
        let receipt = self
            .observations
            .get(name)
            .ok_or_else(|| unsupported(path))?;
        if !matches!(receipt, ObjectReceipt::Movement { .. }) {
            return Err(unsupported(path));
        }
        Ok(super::numeric::aggregate_count(
            Selector::ReceiptMembers {
                receipt: receipt.clone(),
                projection: MemberProjection::Before,
            },
            None,
        ))
    }

    pub(super) fn receipt_count(
        &mut self,
        query: &ast::ReceiptCountQuery,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let Some(ObjectReceipt::Movement {
            receipt: MovementReceipt::Discard { receipt },
        }) = self.observations.get(&query.r#in.objects_of)
        else {
            return Err(unsupported(path));
        };
        let ReceiptLocation::Local { id } = receipt.location() else {
            return Err(unsupported(path));
        };
        if self
            .receipt_reads
            .discard_bounds
            .get(id)
            .is_none_or(|bounds| bounds.maximum.is_none_or(|maximum| maximum.0 > 1))
        {
            return Err(unsupported(path));
        }
        let condition = self.receipt_any(query, path)?;
        Ok(Expr::Compute(Box::new(IntOperation::Conditional {
            condition,
            then_value: Expr::Literal(Int(1)),
            else_value: Expr::Literal(Int(0)),
        })))
    }

    pub(super) fn receipt_any(
        &mut self,
        query: &ast::ReceiptCountQuery,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let receipt = self
            .observations
            .get(&query.r#in.objects_of)
            .ok_or_else(|| unsupported(path))?
            .clone();
        let readable = match &receipt {
            ObjectReceipt::Movement {
                receipt: MovementReceipt::Discard { .. },
            } => true,
            // Only cards leaving the deck carry no field modifications to read as LKI.
            ObjectReceipt::Movement {
                receipt: MovementReceipt::Move { receipt },
            } => matches!(
                receipt.location(),
                ReceiptLocation::Local { id } if self.receipt_reads.deck_moves.contains(id)
            ),
            ObjectReceipt::Movement { .. }
            | ObjectReceipt::Batch { .. }
            | ObjectReceipt::TransformBanished { .. }
            | ObjectReceipt::TransformCreated { .. }
            | ObjectReceipt::Token { .. }
            | ObjectReceipt::Flip { .. }
            | ObjectReceipt::Look { .. } => false,
        };
        if !readable {
            return Err(unsupported(path));
        }
        let member = Binding::new(self.allocator.id(path)?, self.card_allocator.scope(path)?);
        let predicate = super::selection::view_filter(
            &super::selection::ObjectFilter {
                kind: query.kind.as_ref().map(super::search::card_kind),
                trait_id: query.r#trait.as_deref(),
                class: query.class.as_deref(),
                not_class: query.not_class.as_deref(),
                name: query.name.as_deref(),
                contains: query.name_contains.as_deref(),
                original_cost: [None, None, query.orig_cost_ge],
                ..super::selection::ObjectFilter::default()
            },
            ObjectView::Snapshot {
                value: Box::new(Expr::Bound(member)),
            },
        );
        Ok(Expr::Compute(Box::new(Predicate::ReceiptAny {
            receipt,
            projection: MemberProjection::Before,
            member,
            predicate,
        })))
    }
}
