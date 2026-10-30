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
            ast::Condition::EnteredFrom(condition) => self.entered_from(condition, path)?,
            ast::Condition::EnteredBy(condition) => self.entered_by(condition, path)?,
            ast::Condition::PlayedFrom(condition) => self.played_from(condition, path)?,
            ast::Condition::Combo(threshold) => return Self::combo(*threshold, path),
            ast::Condition::Exists(query) => return self.public_exists(query, path),
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
            ast::Condition::Did(name) => return self.move_success(name, path),
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

    fn entered_from(
        &self,
        condition: &ast::EntryOriginCondition,
        path: &str,
    ) -> Result<Predicate, LoweringError> {
        Ok(Predicate::Equal {
            operands: Box::new(super::super::ValueEquality::EntryOrigin {
                left: Expr::Frozen(Box::new(super::super::EntryOriginFields::EnterFieldFrom {
                    event: self.entry_event.ok_or_else(|| unsupported(path))?,
                })),
                right: Expr::Literal(super::super::EntryOrigin::Zone {
                    zone: match condition.zone {
                        ast::EntryOriginZone::Hand => ZoneKind::Hand,
                    },
                }),
            }),
        })
    }

    fn entered_by(
        &self,
        condition: &ast::EntryCondition,
        path: &str,
    ) -> Result<Predicate, LoweringError> {
        Ok(Predicate::Equal {
            operands: Box::new(super::super::ValueEquality::EntryMethod {
                left: Expr::Frozen(Box::new(
                    super::super::EntryMethodFields::EnterFieldMethod {
                        event: self.entry_event.ok_or_else(|| unsupported(path))?,
                    },
                )),
                right: Expr::Literal(match condition.by {
                    ast::EntryMethod::Ability => super::super::EntryMethod::AbilityPut,
                    ast::EntryMethod::Play => super::super::EntryMethod::Play,
                }),
            }),
        })
    }

    fn played_from(
        &self,
        condition: &ast::PlayedFromCondition,
        path: &str,
    ) -> Result<Predicate, LoweringError> {
        let event = self.entry_event.ok_or_else(|| unsupported(path))?;
        let ast::PlayedZone::Ex = condition.from;
        let method = Predicate::Equal {
            operands: Box::new(super::super::ValueEquality::EntryMethod {
                left: Expr::Frozen(Box::new(
                    super::super::EntryMethodFields::EnterFieldMethod { event },
                )),
                right: Expr::Literal(super::super::EntryMethod::Play),
            }),
        };
        let origin = Predicate::Equal {
            operands: Box::new(super::super::ValueEquality::EntryOrigin {
                left: Expr::Frozen(Box::new(super::super::EntryOriginFields::EnterFieldFrom {
                    event,
                })),
                right: Expr::Literal(super::super::EntryOrigin::Zone { zone: ZoneKind::Ex }),
            }),
        };
        Ok(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(method)),
                Expr::Compute(Box::new(origin)),
            ],
        })
    }

    fn move_success(&self, name: &str, path: &str) -> Result<Expr<Bool>, LoweringError> {
        let receipt = self
            .observations
            .get(name)
            .ok_or_else(|| unsupported(path))?;
        if !matches!(
            receipt,
            super::super::ObjectReceipt::Movement {
                receipt: super::super::MovementReceipt::Move { .. }
            }
        ) {
            return Err(unsupported(path));
        }
        Ok(Expr::Compute(Box::new(Predicate::Exists {
            objects: Selector::ReceiptMembers {
                receipt: receipt.clone(),
                projection: super::super::MemberProjection::Before,
            },
        })))
    }

    fn combo(threshold: u64, path: &str) -> Result<Expr<Bool>, LoweringError> {
        if threshold == 0 {
            return Err(unsupported(path));
        }
        Ok(Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Live(Box::new(NatRead::Combo {
                    player: PlayerRef::Controller {},
                    execution: super::super::ExecutionRef::Current {},
                })),
            })),
            comparison: Comparison::GreaterEqual,
            right: Expr::Literal(Int(
                i64::try_from(threshold).map_err(|_private| unsupported(path))?
            )),
        })))
    }

    fn played_count(
        &mut self,
        history: &ast::PlayedHistory,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let player = history.who.as_ref().map(|side| match side {
            ast::LeaderSide::Controller => PlayerRef::Controller {},
            ast::LeaderSide::Opponent => super::super::events::timing::opponent(),
        });
        let event = Binding::new(self.allocator.id(path)?, self.scope);
        Ok(Expr::Compute(Box::new(IntOperation::FromNat {
            value: Expr::Live(Box::new(NatRead::HistoryCount {
                query: super::super::history_turn::played_query(event, player),
                count: super::super::HistoryCount::Facts,
            })),
        })))
    }

    fn public_exists(
        &mut self,
        query: &ast::PublicQuery,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let binding = Binding::new(self.allocator.id(path)?, self.card_allocator.scope(path)?);
        let mut objects = super::costs::public_query(query, binding);
        if query.other {
            objects = Selector::Difference {
                input: Box::new(objects),
                except: Box::new(Selector::Objects {
                    objects: vec![super::super::ObjectOperand::SourceHolder {}],
                }),
            };
        }
        Ok(Expr::Compute(Box::new(Predicate::Exists { objects })))
    }

    pub(super) fn number(
        &mut self,
        value: &ast::Number,
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        match value {
            ast::Number::HistoryCount { history_count } => self.played_count(history_count, path),
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
            ast::Number::Count { count } => self.zone_count(count, path),
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

    fn zone_count(
        &mut self,
        count: &ast::CountQuery,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let filtered = count.kind.is_some()
            || count.r#trait.is_some()
            || count.name.is_some()
            || count.name_contains.is_some()
            || count.orig_cost_eq.is_some()
            || count.token.is_some()
            || count.not_token.is_some();
        if !matches!(
            (count.token, count.not_token),
            (None | Some(true), None) | (None, Some(true))
        ) {
            return Err(unsupported(path));
        }
        if matches!(count.zone, ast::CountZone::Hand | ast::CountZone::Deck) && filtered {
            return Err(unsupported(path));
        }
        let binding = Binding::new(self.allocator.id(path)?, self.scope);
        if count
            .orig_cost_eq
            .is_some_and(|cost| i64::try_from(cost).is_err())
        {
            return Err(unsupported(path));
        }
        let predicate =
            super::selection::field_filter(&super::selection::ObjectFilter::from(count), binding);
        Ok(Expr::Compute(Box::new(IntOperation::FromNat {
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
                        ast::CountZone::Field => ZoneKind::Field,
                        ast::CountZone::Ex => ZoneKind::Ex,
                        ast::CountZone::Deck => ZoneKind::Deck,
                        ast::CountZone::Hand => ZoneKind::Hand,
                        ast::CountZone::Cemetery => ZoneKind::Cemetery,
                    },
                    view: SelectionView::Current {},
                    binding,
                    predicate: Box::new(predicate),
                },
            })),
        })))
    }
}
