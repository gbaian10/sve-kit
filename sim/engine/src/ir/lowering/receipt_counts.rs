use super::{Binding, BodyContext, Bool, Expr, Int, LoweringError, ast, unsupported};
use crate::ir::{
    Cardinality, CharacteristicView, Comparison, IntFrozen, IntOperation, MemberProjection,
    MovementReceipt, Nat, NatOperation, NumberRead, ObjectProperty, ObjectReceipt, ObjectView,
    Predicate, ReceiptLocation, RuntimeObjectView, Selector,
};

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

    pub(super) fn receipt_original_costs(
        &self,
        query: &ast::BeforeObjects,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let receipt = self
            .observations
            .get(&query.objects_of)
            .ok_or_else(|| unsupported(path))?;
        if !matches!(receipt, ObjectReceipt::Movement { receipt: MovementReceipt::Move { receipt } }
            if matches!(receipt.location(), ReceiptLocation::Local { id } if self.receipt_reads.deck_cemetery.contains(id)))
        {
            return Err(unsupported(path));
        }
        Ok(super::numeric::aggregate_count(
            Selector::ReceiptMembers {
                receipt: receipt.clone(),
                projection: MemberProjection::Before,
            },
            Some(ObjectProperty::Cost),
        ))
    }

    pub(super) fn count_of(
        &mut self,
        value: &ast::CountOf,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        match value {
            ast::CountOf::Named(name) => self.named_count_of(name, path),
            ast::CountOf::Filtered(query) => self.filtered_count_of(query, path),
        }
    }

    fn filtered_count_of(
        &mut self,
        query: &ast::FilteredCountOf,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let receipt = self
            .observations
            .get(&query.receipt)
            .ok_or_else(|| unsupported(path))?
            .clone();
        let ObjectReceipt::Movement {
            receipt: MovementReceipt::Move { receipt: reference },
        } = &receipt
        else {
            return Err(unsupported(path));
        };
        if !matches!(reference.location(), ReceiptLocation::Local { id } if self.receipt_reads.deck_cemetery.contains(id))
        {
            return Err(unsupported(path));
        }
        let (member, mut predicate) = self.receipt_filter(
            &super::selection::ObjectFilter {
                trait_id: query.filter.r#trait.as_deref(),
                ..super::selection::ObjectFilter::default()
            },
            path,
        )?;
        if let Some(number) = &query.filter.orig_cost_eq {
            let cost = Expr::Compute(Box::new(IntOperation::InformationOrZero {
                information: NumberRead::Cost {
                    object: ObjectView::Snapshot {
                        value: Box::new(Expr::Bound(member)),
                    },
                    view: CharacteristicView::Original,
                },
            }));
            let comparison = if let Some(right) = self.declared_natural(number) {
                Predicate::CompareNat {
                    left: Expr::Compute(Box::new(NatOperation::CheckedInt { value: cost })),
                    comparison: Comparison::Equal,
                    right,
                }
            } else {
                Predicate::CompareInt {
                    left: cost,
                    comparison: Comparison::Equal,
                    right: self.number(number, path, &mut super::BTreeSet::new())?,
                }
            };
            predicate = Expr::Compute(Box::new(Predicate::And {
                predicates: vec![predicate, Expr::Compute(Box::new(comparison))],
            }));
        }
        Ok(Expr::Compute(Box::new(IntOperation::FromNat {
            value: Expr::Compute(Box::new(NatOperation::ReceiptCount {
                receipt,
                projection: MemberProjection::Before,
                member,
                predicate,
            })),
        })))
    }

    fn named_count_of(&self, name: &str, path: &str) -> Result<Expr<Int>, LoweringError> {
        let receipt = self
            .observations
            .get(name)
            .ok_or_else(|| unsupported(path))?;
        if !matches!(
            receipt,
            ObjectReceipt::Movement { .. } | ObjectReceipt::Flip { .. }
        ) {
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

    pub(super) fn is_mill_receipt(&self, name: &str) -> bool {
        matches!(self.observations.get(name),
            Some(ObjectReceipt::Movement { receipt: MovementReceipt::Move { receipt } })
                if matches!(receipt.location(), ReceiptLocation::Local { id }
                    if self.receipt_reads.deck_cemetery.contains(id)))
    }

    pub(super) fn moved_original_cost(
        &self,
        name: &str,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let receipt = self
            .observations
            .get(name)
            .ok_or_else(|| unsupported(path))?;
        Ok(Expr::Compute(Box::new(IntOperation::Sum {
            objects: Selector::ReceiptMembers {
                receipt: receipt.clone(),
                projection: MemberProjection::Before,
            },
            property: super::super::NumericProperty::OriginalCost,
        })))
    }

    pub(super) fn banish_receipt(
        &self,
        name: &str,
        path: &str,
    ) -> Result<&ObjectReceipt, LoweringError> {
        self.observations
            .get(name)
            .filter(|receipt| {
                matches!(
                    receipt,
                    ObjectReceipt::Movement {
                        receipt: MovementReceipt::Banish { .. }
                    }
                )
            })
            .ok_or_else(|| unsupported(path))
    }

    pub(super) fn receipt_count(
        &mut self,
        query: &ast::ReceiptCountQuery,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        if let Some(receipt) = self.observations.get(&query.r#in.objects_of).cloned()
            && (matches!(
                receipt,
                ObjectReceipt::Fusion { .. } | ObjectReceipt::Reveal { .. }
            ) || self.is_mill_receipt(&query.r#in.objects_of)
                || self
                    .public_observation(&query.r#in.objects_of, path)
                    .is_ok())
        {
            let (member, predicate) = self.count_query_filter(query, path)?;
            return Ok(Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Compute(Box::new(NatOperation::ReceiptCount {
                    receipt,
                    projection: MemberProjection::Before,
                    member,
                    predicate,
                })),
            })));
        }
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
            ObjectReceipt::Fusion { .. }
            | ObjectReceipt::Movement {
                receipt: MovementReceipt::Discard { .. },
            } => true,
            // Field movements expose only their retained immutable printing.
            ObjectReceipt::Movement {
                receipt: MovementReceipt::Move { receipt },
            } => matches!(
                receipt.location(),
                ReceiptLocation::Local { id } if self.receipt_reads.deck_moves.contains(id)
                    || self.receipt_reads.field_cemetery.contains(id)
            ),
            ObjectReceipt::Look { receipt } => {
                matches!(receipt.location(), ReceiptLocation::Local { id } if self.receipt_reads.published_looks.contains(id))
            }
            ObjectReceipt::Movement { .. }
            | ObjectReceipt::Random { .. }
            | ObjectReceipt::Batch { .. }
            | ObjectReceipt::TransformBanished { .. }
            | ObjectReceipt::TransformCreated { .. }
            | ObjectReceipt::Token { .. }
            | ObjectReceipt::Flip { .. }
            | ObjectReceipt::RevealUntil { .. }
            | ObjectReceipt::Reveal { .. } => false,
        };
        if !readable {
            return Err(unsupported(path));
        }
        if matches!(&receipt, ObjectReceipt::Movement { receipt: MovementReceipt::Move { receipt } }
            if matches!(receipt.location(), ReceiptLocation::Local { id } if self.receipt_reads.field_cemetery.contains(id)))
            && (query.kind.is_some()
                || query.keyword.is_some()
                || query.r#trait.is_some()
                || query.class.is_some()
                || query.not_class.is_some()
                || query.name.is_some()
                || query.name_contains.is_some())
        {
            return Err(unsupported(path));
        }
        let (member, predicate) = self.count_query_filter(query, path)?;
        Ok(Expr::Compute(Box::new(Predicate::ReceiptAny {
            receipt,
            projection: MemberProjection::Before,
            member,
            predicate,
        })))
    }

    fn count_query_filter(
        &mut self,
        query: &ast::ReceiptCountQuery,
        path: &str,
    ) -> Result<(Binding<RuntimeObjectView>, Expr<Bool>), LoweringError> {
        let name = match &query.name {
            Some(ast::ReceiptName::Literal(name)) => Some(name.as_str()),
            Some(ast::ReceiptName::Bound { .. }) | None => None,
        };
        let (member, predicate) = self.receipt_filter(
            &super::selection::ObjectFilter {
                kind: query.kind.as_ref().map(super::search::card_kind),
                keyword: query.keyword.map(|keyword| keyword.0),
                trait_id: query.r#trait.as_deref(),
                class: query.class.as_deref(),
                not_class: query.not_class.as_deref(),
                name,
                contains: query.name_contains.as_deref(),
                original_cost: [query.orig_cost_eq, None, query.orig_cost_ge],
                ..super::selection::ObjectFilter::default()
            },
            path,
        )?;
        let Some(ast::ReceiptName::Bound { bound }) = &query.name else {
            return Ok((member, predicate));
        };
        let binding = *self
            .declared_names
            .get(bound)
            .ok_or_else(|| unsupported(path))?;
        let matched = Expr::Compute(Box::new(Predicate::NameEquals {
            object: ObjectView::Snapshot {
                value: Box::new(Expr::Bound(member)),
            },
            name: Expr::Bound(binding),
        }));
        Ok((
            member,
            Expr::Compute(Box::new(Predicate::And {
                predicates: vec![predicate, matched],
            })),
        ))
    }

    fn receipt_filter(
        &mut self,
        filter: &super::selection::ObjectFilter<'_>,
        path: &str,
    ) -> Result<(Binding<RuntimeObjectView>, Expr<Bool>), LoweringError> {
        let member = Binding::new(self.allocator.id(path)?, self.card_allocator.scope(path)?);
        let predicate = super::selection::view_filter(
            filter,
            ObjectView::Snapshot {
                value: Box::new(Expr::Bound(member)),
            },
        );
        Ok((member, predicate))
    }
}
