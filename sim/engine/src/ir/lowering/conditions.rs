use super::super::{Comparison, IntRead, PlayerResource};
use super::{
    Arithmetic, BTreeSet, Binding, BodyContext, Bool, Cardinality, Expr, FaceId, Int, IntOperation,
    LoweringError, Nat, NatRead, PlayerRef, PlayerRelation, PlayerSet, Predicate, SelectionView,
    Selector, SourceSpan, TextQuantifier, ZoneKind, ast, unsupported,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Body expressions share the ability binding and definition environment."
)]
impl BodyContext<'_> {
    pub(super) fn condition(
        &mut self,
        value: &ast::Condition,
        source: &SourceSpan,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        Ok(Expr::Compute(Box::new(match value {
            ast::Condition::Exists(query) => {
                if !matches!(query.zone, ast::PublicZone::Field) {
                    return Err(unsupported(path));
                }
                Predicate::Exists {
                    objects: super::costs::query(query, &mut self.allocator, path)?,
                }
            }
            ast::Condition::Awakened(true) => Predicate::CompareInt {
                left: Expr::Live(Box::new(IntRead::PlayerResource {
                    player: PlayerRef::Controller {},
                    property: PlayerResource::PpMaximum,
                })),
                comparison: Comparison::GreaterEqual,
                right: Expr::Literal(Int(7)),
            },
            ast::Condition::LeaderLife(threshold) => {
                let (comparison, amount) = match (threshold.eq, threshold.le, threshold.ge) {
                    (Some(amount), None, None) => (Comparison::Equal, amount),
                    (None, Some(amount), None) => (Comparison::LessEqual, amount),
                    (None, None, Some(amount)) => (Comparison::GreaterEqual, amount),
                    _ => return Err(unsupported(path)),
                };
                Predicate::CompareInt {
                    left: self.number(
                        &ast::Number::LeaderLife {
                            leader_life: threshold.who,
                        },
                        path,
                        &mut BTreeSet::new(),
                    )?,
                    comparison,
                    right: Expr::Literal(Int(
                        i64::try_from(amount).map_err(|_private| unsupported(path))?
                    )),
                }
            }

            ast::Condition::Crimson(true) => {
                return Ok(Expr::Live(Box::new(super::super::BoolRead::Crimson {
                    player: PlayerRef::Controller {},
                })));
            }
            ast::Condition::Awakened(false) | ast::Condition::Crimson(false) => {
                return Err(unsupported(path));
            }
            ast::Condition::All(items) | ast::Condition::Any(items) => {
                let predicates = items
                    .iter()
                    .map(|item| self.condition(item, source, path))
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(value, ast::Condition::All(_)) {
                    Predicate::And { predicates }
                } else {
                    Predicate::Or { predicates }
                }
            }
            ast::Condition::Not(item) => Predicate::Not {
                predicate: self.condition(item, source, path)?,
            },
            ast::Condition::Cmp(value) => Predicate::CompareInt {
                left: self.number(&value.left, path, &mut BTreeSet::new())?,
                comparison: match value.op {
                    ast::Comparison::Eq => Comparison::Equal,
                    ast::Comparison::Ne => Comparison::NotEqual,
                    ast::Comparison::Lt => Comparison::Less,
                    ast::Comparison::Le => Comparison::LessEqual,
                    ast::Comparison::Gt => Comparison::Greater,
                    ast::Comparison::Ge => Comparison::GreaterEqual,
                },
                right: self.number(&value.right, path, &mut BTreeSet::new())?,
            },
            ast::Condition::Paid(paid) => {
                if let Some((name, receipt)) = &self.local_payment {
                    if name != &paid.receipt {
                        return Err(unsupported(path));
                    }
                    return Ok(Expr::Frozen(Box::new(
                        super::super::BoolFrozen::PaymentComplete {
                            receipt: super::super::PaymentReference::Local {
                                receipt: super::super::ReceiptReference::new(*receipt),
                            },
                        },
                    )));
                }
                let (name, receipt) = self
                    .payment_result
                    .as_ref()
                    .ok_or_else(|| unsupported(path))?;
                if name != &paid.receipt {
                    return Err(unsupported(path));
                }
                return Ok(Expr::Frozen(Box::new(
                    super::super::BoolFrozen::PaymentComplete {
                        receipt: super::super::PaymentReference::CardPlay {
                            face: FaceId(source.face.clone()),
                            receipt: *receipt,
                        },
                    },
                )));
            }
        })))
    }

    pub(super) fn number(
        &mut self,
        value: &ast::Number,
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        match value {
            ast::Number::Literal(value) => Ok(Expr::Literal(Int(*value))),
            ast::Number::Resolution { at, value } => {
                let ast::ResolutionTime::Resolution = at;
                self.resolution_count(value, path, active)
            }
            ast::Number::Selection {
                at,
                selection,
                value,
            } => {
                let ast::SelectionTime::Selection = at;
                self.selected_stat(selection, value, path)
            }
            ast::Number::LeaderLife { leader_life } => {
                Ok(Expr::Live(Box::new(IntRead::PlayerResource {
                    player: match leader_life {
                        ast::LeaderSide::Controller => PlayerRef::Controller {},
                        ast::LeaderSide::Opponent => super::super::events::timing::opponent(),
                    },
                    property: PlayerResource::Life,
                })))
            }
            ast::Number::Alias(name) => {
                if !active.insert(name.clone()) {
                    return Err(unsupported(path));
                }
                let definition = self
                    .definitions
                    .get(name)
                    .ok_or_else(|| unsupported(path))?;
                let result = self.number(definition, path, active);
                active.remove(name);
                result
            }
            ast::Number::Minus { minus } => {
                let [left, right] = minus.as_ref();
                Ok(Expr::Compute(Box::new(IntOperation::Arithmetic {
                    operation: Arithmetic::Subtract,
                    arguments: vec![
                        self.number(left, path, active)?,
                        self.number(right, path, active)?,
                    ],
                })))
            }
            ast::Number::Count { count } => Ok(Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Live(Box::new(NatRead::ZoneCount {
                    selector: Selector::Zone {
                        players: PlayerSet {
                            relation: match count.who {
                                ast::LeaderSide::Controller => PlayerRelation::SelfPlayer,
                                ast::LeaderSide::Opponent => PlayerRelation::Opponents,
                            },
                            relative_to: PlayerRef::Controller {},
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone: match count.zone {
                            ast::CountZone::Deck => ZoneKind::Deck,
                            ast::CountZone::Hand => ZoneKind::Hand,
                            ast::CountZone::Cemetery => ZoneKind::Cemetery,
                        },
                        view: SelectionView::Current {},
                        binding: Binding::new(self.allocator.id(path)?, self.scope),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    },
                })),
            }))),
            ast::Number::Resource { resource } => {
                Ok(Expr::Live(Box::new(IntRead::PlayerResource {
                    player: match resource.who {
                        ast::LeaderSide::Controller => PlayerRef::Controller {},
                        ast::LeaderSide::Opponent => super::super::events::timing::opponent(),
                    },
                    property: match resource.kind {
                        ast::PpResource::Pp => PlayerResource::PpCurrent,
                        ast::PpResource::PpMax => PlayerResource::PpMaximum,
                    },
                })))
            }
        }
    }
}
