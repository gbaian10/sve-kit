use super::super::{
    CharacteristicView, Comparison, CostSpec, CounterKind, IntFrozen, IntRead, NatFrozen,
    NumberRead, ObjectView, PlayerResource, Precondition, PreconditionKind, PreconditionReason,
    SnapshotFields, Stat,
};
use super::{
    Arithmetic, BTreeSet, Binding, BodyContext, Bool, Cardinality, CounterReadView, Expr, FaceId,
    Int, IntOperation, LoweringError, Nat, NatRead, PlayerRef, PlayerRelation, PlayerSet,
    Predicate, SelectionView, Selector, SourceSpan, TextQuantifier, ZoneKind, ast, unsupported,
};

pub(super) fn qualify_costs(
    costs: &mut CostSpec,
    predicate: Option<Expr<Bool>>,
    path: &str,
) -> Result<(), LoweringError> {
    let Some(predicate) = predicate else {
        return Ok(());
    };
    let CostSpec::Ordered { steps } = costs else {
        return Err(unsupported(path));
    };
    let first = steps.first_mut().ok_or_else(|| unsupported(path))?;
    first.instruction.precondition = Some(Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        protects: first.instruction.id,
        reason: PreconditionReason::CardText {},
        predicate,
    });
    Ok(())
}

pub(super) fn leader_life_condition(
    threshold: &ast::PlayerThreshold,
    path: &str,
) -> Result<Predicate, LoweringError> {
    let (comparison, amount) = match (threshold.eq, threshold.le, threshold.ge) {
        (Some(amount), None, None) => (Comparison::Equal, amount),
        (None, Some(amount), None) => (Comparison::LessEqual, amount),
        (None, None, Some(amount)) => (Comparison::GreaterEqual, amount),
        _ => return Err(unsupported(path)),
    };
    Ok(Predicate::CompareInt {
        left: Expr::Live(Box::new(IntRead::PlayerResource {
            player: match threshold.who {
                ast::LeaderSide::Controller => PlayerRef::Controller {},
                ast::LeaderSide::Opponent => super::super::events::timing::opponent(),
            },
            property: PlayerResource::Life,
        })),
        comparison,
        right: Expr::Literal(Int(
            i64::try_from(amount).map_err(|_private| unsupported(path))?
        )),
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Body expressions share the ability binding and definition environment."
)]
impl BodyContext<'_> {
    pub(super) fn attack_target_objects(&mut self, path: &str) -> Result<Selector, LoweringError> {
        let Some(super::super::events::subject::SubjectFields::Attack(event)) = self.subject_event
        else {
            return Err(unsupported(path));
        };
        let candidate = Binding::new(self.allocator.id(path)?, self.scope);
        Ok(super::super::events::attack::target_selector(
            event, candidate,
        ))
    }

    pub(super) fn activation_qualification(
        &mut self,
        activated: Option<&ast::Activate>,
        source: &SourceSpan,
        path: &str,
    ) -> Result<Option<Expr<Bool>>, LoweringError> {
        activated
            .and_then(|head| head.usable_if.as_ref())
            .map(|condition| self.qualification(condition, source, path))
            .transpose()
    }

    pub(super) fn qualification(
        &mut self,
        condition: &ast::Condition,
        source: &SourceSpan,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let predicate = self.condition(condition, source, path)?;
        // A use prerequisite cannot depend on a later resolution capture or choice.
        if !self.resolution_captures.is_empty() {
            return Err(unsupported(path));
        }
        Ok(predicate)
    }

    fn frozen_population(
        &mut self,
        threshold: u64,
        spells: bool,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let count = ast::Number::Count {
            count: ast::CountQuery {
                face_up: None,
                class: None,
                not_class: None,
                other: false,
                who: ast::Side::Controller,
                zone: ast::CountZone::Cemetery,
                kind: spells.then_some(ast::SearchKind::Spell),
                keyword: None,
                r#trait: None,
                name: None,
                name_contains: None,
                orig_cost_eq: None,
                token: None,
                not_token: None,
            },
        };
        Ok(Expr::Compute(Box::new(Predicate::CompareInt {
            left: self.resolution_count(&count, path, &mut BTreeSet::new())?,
            comparison: Comparison::GreaterEqual,
            right: Expr::Literal(Int(
                i64::try_from(threshold).map_err(|_private| unsupported(path))?
            )),
        })))
    }

    fn history_condition(
        &mut self,
        history: &ast::HistoryQuery,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let threshold = history.at_least.unwrap_or(1);
        Ok(Expr::Compute(Box::new(Predicate::CompareInt {
            left: self.history_number(history, path)?,
            comparison: Comparison::GreaterEqual,
            right: Expr::Literal(Int(
                i64::try_from(threshold).map_err(|_private| unsupported(path))?
            )),
        })))
    }

    // The source's original generation must still be in its controller's EX at resolution.
    fn source_in_ex(
        &mut self,
        location: &ast::PlayRestrictionZone,
        path: &str,
    ) -> Result<Predicate, LoweringError> {
        if location.r#ref != "this" || location.zone != "ex" {
            return Err(unsupported(path));
        }
        Ok(Predicate::Exists {
            objects: Selector::Intersection {
                sets: vec![
                    super::numeric::holder(),
                    Selector::Zone {
                        players: PlayerSet {
                            relation: PlayerRelation::SelfPlayer,
                            relative_to: PlayerRef::Controller {},
                            cardinality: Cardinality {
                                minimum: Nat(0),
                                maximum: None,
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone: ZoneKind::Ex,
                        view: SelectionView::Current {},
                        binding: Binding::new(self.allocator.id(path)?, self.scope),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    },
                ],
            },
        })
    }

    pub(super) fn condition(
        &mut self,
        value: &ast::Condition,
        source: &SourceSpan,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        Ok(Expr::Compute(Box::new(match value {
            ast::Condition::MyTurn(true) => Predicate::PlayerEquals {
                left: Expr::Literal(PlayerRef::Controller {}),
                right: Expr::Literal(PlayerRef::Active {}),
            },
            ast::Condition::MyTurn(false) => return Err(unsupported(path)),
            ast::Condition::InZone(location) => self.source_in_ex(location, path)?,
            ast::Condition::Is(condition) => {
                let ast::AttackTargetReference::AttackTarget = condition.reference;
                let ast::FollowerKind::Follower = condition.kind;
                Predicate::Exists {
                    objects: self.attack_target_objects(path)?,
                }
            }
            ast::Condition::History(history) => return self.history_condition(history, path),
            ast::Condition::EnteredFrom(condition) => return self.entered_from(condition, path),
            ast::Condition::EnteredBy(condition) => self.entered_by(condition, path)?,
            ast::Condition::PlayedFrom(condition) => self.played_from(condition, path)?,
            ast::Condition::Combo(threshold) => return Self::combo(*threshold, path),
            ast::Condition::Spellchain(threshold) => {
                return self.frozen_population(*threshold, true, path);
            }
            ast::Condition::Necro(threshold) => {
                return self.frozen_population(*threshold, false, path);
            }
            ast::Condition::Exists(query) => return self.public_exists(query, path),
            ast::Condition::Standing(ast::This::This) => Predicate::PostureEquals {
                left: Expr::Live(Box::new(super::super::PostureRead::Posture {
                    object: ObjectView::Current {
                        object: Box::new(super::super::ObjectOperand::SourceHolder {}),
                    },
                })),
                right: Expr::Literal(super::super::Posture::Standing),
            },
            ast::Condition::Awakened(true) => Predicate::CompareInt {
                left: Expr::Live(Box::new(IntRead::PlayerResource {
                    player: PlayerRef::Controller {},
                    property: PlayerResource::PpMaximum,
                })),
                comparison: Comparison::GreaterEqual,
                right: Expr::Literal(Int(7)),
            },
            ast::Condition::LeaderLife(threshold) => leader_life_condition(threshold, path)?,

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
            ast::Condition::Cmp(value) => return self.comparison_condition(value, path),
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

    fn comparison_condition(
        &mut self,
        value: &ast::BodyComparison,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        if let (ast::Number::ReceiptCount { count }, ast::Number::Literal(threshold)) =
            (&value.left, &value.right)
        {
            let exists = matches!(
                (value.op, *threshold),
                (ast::Comparison::Ge, 1) | (ast::Comparison::Gt | ast::Comparison::Ne, 0)
            );
            let absent = matches!(
                (value.op, *threshold),
                (ast::Comparison::Lt, 1) | (ast::Comparison::Le | ast::Comparison::Eq, 0)
            );
            if exists || absent {
                let predicate = self.receipt_any(count, path)?;
                return Ok(if exists {
                    predicate
                } else {
                    Expr::Compute(Box::new(Predicate::Not { predicate }))
                });
            }
        }
        Ok(Expr::Compute(Box::new(Predicate::CompareInt {
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
        })))
    }

    pub(super) fn entered_from(
        &self,
        condition: &ast::EntryOriginCondition,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let zone = match condition.zone {
            ast::EntryOriginZone::Hand => ZoneKind::Hand,
            ast::EntryOriginZone::Deck => ZoneKind::Deck,
            ast::EntryOriginZone::Cemetery => ZoneKind::Cemetery,
        };
        let event = match condition.reference {
            ast::EntryOriginReference::This => self.entry_event,
            ast::EntryOriginReference::It => match self.subject_event {
                Some(super::super::events::subject::SubjectFields::Entry(event)) => Some(event),
                _ => return Err(unsupported(path)),
            },
        };
        if let Some(event) = event {
            return Ok(Expr::Compute(Box::new(Predicate::Equal {
                operands: Box::new(super::super::ValueEquality::EntryOrigin {
                    left: Expr::Frozen(Box::new(super::super::EntryOriginFields::EnterFieldFrom {
                        event,
                    })),
                    right: Expr::Literal(super::super::EntryOrigin::Zone { zone }),
                }),
            })));
        }
        if self.evolution.is_none()
            || zone != ZoneKind::Cemetery
            || !matches!(condition.reference, ast::EntryOriginReference::This)
        {
            return Err(unsupported(path));
        }
        Ok(Expr::Live(Box::new(
            super::super::BoolRead::FieldEntryOrigin {
                object: super::super::ObjectOperand::SourceHolder {},
                zone,
            },
        )))
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

    fn history_number(
        &mut self,
        history: &ast::HistoryQuery,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let query = self.history_query(history, path)?;
        Ok(Expr::Compute(Box::new(IntOperation::FromNat {
            value: Expr::Live(Box::new(NatRead::HistoryCount {
                query,
                count: match history.distinct {
                    Some(ast::HistoryDistinct::Subjects) => super::super::HistoryCount::Subjects,
                    None | Some(ast::HistoryDistinct::Facts) => super::super::HistoryCount::Facts,
                },
            })),
        })))
    }

    fn history_query(
        &mut self,
        history: &ast::HistoryQuery,
        path: &str,
    ) -> Result<super::super::HistoryQuery, LoweringError> {
        use super::super::history_turn::{SubjectHistoryKind, subject_query};
        let id = self.allocator.id(path)?;
        let own = history
            .who
            .as_ref()
            .map(|side| matches!(side, ast::LeaderSide::Controller));
        if matches!(history.event, ast::HistoryEvent::Played) {
            if history.sel.is_some()
                || history.from.is_some()
                || history.to.is_some()
                || matches!(history.distinct, Some(ast::HistoryDistinct::Subjects))
            {
                return Err(unsupported(path));
            }
            return Ok(super::super::history_turn::played_query(
                Binding::new(id, self.scope),
                own.map(|own| {
                    if own {
                        PlayerRef::Controller {}
                    } else {
                        super::super::events::timing::opponent()
                    }
                }),
            ));
        }
        let mut filter = super::movement::movement_filter(own, false, history.sel.as_ref(), false);
        let kind = match history.event {
            ast::HistoryEvent::Attacked => SubjectHistoryKind::Attack,
            ast::HistoryEvent::Evolved => SubjectHistoryKind::Evolve,
            ast::HistoryEvent::Destroyed => SubjectHistoryKind::Destroy,
            ast::HistoryEvent::LeftField => SubjectHistoryKind::Leave,
            ast::HistoryEvent::Moved => {
                if history.who.is_some() {
                    return Err(unsupported(path));
                }
                let from = history.from.as_ref().ok_or_else(|| unsupported(path))?;
                filter.own = match from {
                    ast::HistoryOrigin::Zone(ast::FieldOrigin::Field) => None,
                    ast::HistoryOrigin::Owned {
                        who,
                        zone: ast::FieldOrigin::Field,
                    } => Some(matches!(who, ast::LeaderSide::Controller)),
                };
                match history.to {
                    Some(ast::DepartureDestination::Cemetery) => SubjectHistoryKind::Cemetery,
                    Some(ast::DepartureDestination::Hand) => SubjectHistoryKind::Hand,
                    None => SubjectHistoryKind::Leave,
                }
            }
            ast::HistoryEvent::Played => return Err(unsupported(path)),
        };
        if !matches!(history.event, ast::HistoryEvent::Moved)
            && (history.from.is_some() || history.to.is_some())
        {
            return Err(unsupported(path));
        }
        Ok(subject_query(kind, id, self.scope, &filter))
    }

    fn public_exists(
        &mut self,
        query: &ast::ExistsQuery,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let binding = Binding::new(self.allocator.id(path)?, self.card_allocator.scope(path)?);
        let ast::ExistsQuery::Public(query) = query else {
            let ast::ExistsQuery::Selected(query) = query else {
                return Err(unsupported(path));
            };
            let reference = self
                .targets
                .get(&query.reference)
                .copied()
                .filter(|_| !self.observed_targets.contains(&query.reference))
                .ok_or_else(|| unsupported(path))?;
            return Ok(Expr::Compute(Box::new(Predicate::Exists {
                objects: Selector::Filter {
                    input: Box::new(Selector::Bound { reference }),
                    binding,
                    predicate: Box::new(super::selection::field_filter(
                        &super::selection::ObjectFilter::from(query),
                        binding,
                    )),
                },
            })));
        };
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

    fn body_life(
        &mut self,
        hp_of: &ast::BodyStatReference,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        match hp_of {
            ast::BodyStatReference::This => Ok(this_number(
                &ast::This::This,
                Some(Stat::Life),
                CharacteristicView::Effective,
            )),
            ast::BodyStatReference::AttackTarget => {
                Ok(Expr::Compute(Box::new(IntOperation::Sum {
                    objects: self.attack_target_objects(path)?,
                    property: super::super::NumericProperty::Life,
                })))
            }
        }
    }

    pub(super) fn number(
        &mut self,
        value: &ast::Number,
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        match value {
            ast::Number::Counter { counter } => Ok(counter_number(counter, self.counter_read_view)),
            ast::Number::Power { power_of } => Ok(this_number(
                power_of,
                Some(Stat::Attack),
                CharacteristicView::Effective,
            )),
            ast::Number::Life { hp_of } => self.body_life(hp_of, path),
            ast::Number::Cost { cost_of } => {
                Ok(this_number(cost_of, None, CharacteristicView::Effective))
            }
            ast::Number::OriginalCost { orig_cost_of } => Ok(this_number(
                orig_cost_of,
                None,
                CharacteristicView::Original,
            )),
            ast::Number::HistoryCount { history_count } => {
                if history_count.at_least.is_some() {
                    return Err(unsupported(path));
                }
                self.history_number(history_count, path)
            }
            ast::Number::ReceiptCount { count } => self.receipt_count(count, path),
            ast::Number::CountOf { count_of } => self.count_of(count_of, path),
            ast::Number::Literal(value) => Ok(Expr::Literal(Int(*value))),
            ast::Number::Aggregate { agg } => self.aggregate(agg, path),
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
            ast::Number::Plus { plus } => self.arithmetic(Arithmetic::Add, &plus[..], path, active),
            ast::Number::Minus { minus } => {
                self.arithmetic(Arithmetic::Subtract, &minus[..], path, active)
            }
            ast::Number::Times { times } => {
                self.arithmetic(Arithmetic::Multiply, &times[..], path, active)
            }
            ast::Number::Min { min } => self.minimum(min, path, active),
            ast::Number::Per { per } => self.per(per, path, active),
            ast::Number::Event { event } => self.event_number(event, path),
            ast::Number::Count { count } => self.zone_count(count, path),
            ast::Number::DistinctNames { distinct_names } => super::numeric::distinct_count(
                distinct_names,
                super::super::ObjectProperty::Names,
                Binding::new(self.allocator.id(path)?, self.scope),
                path,
            ),
            ast::Number::DistinctOriginalCosts {
                distinct_orig_costs,
            } => super::numeric::distinct_count(
                distinct_orig_costs,
                super::super::ObjectProperty::Cost,
                Binding::new(self.allocator.id(path)?, self.scope),
                path,
            ),
            ast::Number::TurnOrdinal { turn_number } => Ok(turn_ordinal(turn_number.who)),
            ast::Number::Resource { resource } => {
                Ok(Expr::Live(Box::new(IntRead::PlayerResource {
                    player: match resource.who {
                        ast::LeaderSide::Controller => PlayerRef::Controller {},
                        ast::LeaderSide::Opponent => super::super::events::timing::opponent(),
                    },
                    property: match resource.kind {
                        ast::PpResource::Pp => PlayerResource::PpCurrent,
                        ast::PpResource::PpMax => PlayerResource::PpMaximum,
                        ast::PpResource::Ep => PlayerResource::Ep,
                        ast::PpResource::Sep => PlayerResource::Sep,
                    },
                })))
            }
        }
    }

    fn minimum(
        &mut self,
        operands: &[ast::Number],
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        if operands.is_empty() {
            return Err(unsupported(path));
        }
        self.arithmetic(Arithmetic::Minimum, operands, path, active)
    }

    /// 「N枚につき」 rounds down; the IR also requires a positive literal divisor.
    fn per(
        &mut self,
        per: &ast::Per,
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        let unit = i64::try_from(per.unit)
            .ok()
            .filter(|unit| *unit > 0)
            .ok_or_else(|| unsupported(path))?;
        Ok(Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::FloorDivide,
            arguments: vec![
                self.number(&per.of, path, active)?,
                Expr::Literal(Int(unit)),
            ],
        })))
    }

    /// Only a damage event header provides its actual amount.
    fn event_number(
        &self,
        field: &ast::EventNumber,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let ast::EventNumber::Amount = field;
        self.damage_event
            .map(|event| Expr::Frozen(Box::new(IntFrozen::DamageAmount { event })))
            .ok_or_else(|| unsupported(path))
    }

    fn arithmetic(
        &mut self,
        operation: Arithmetic,
        operands: &[ast::Number],
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        Ok(Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation,
            arguments: operands
                .iter()
                .map(|operand| self.number(operand, path, active))
                .collect::<Result<_, _>>()?,
        })))
    }

    pub(super) fn zone_count(
        &mut self,
        count: &ast::CountQuery,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let filtered = count.face_up.is_some()
            || count.class.is_some()
            || count.not_class.is_some()
            || count.keyword.is_some()
            || count.other
            || count.kind.is_some()
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
        if matches!(
            count.zone,
            ast::CountZone::Hand | ast::CountZone::Deck | ast::CountZone::Banish
        ) && filtered
        {
            return Err(unsupported(path));
        }
        if matches!(count.zone, ast::CountZone::EvolveDeck)
            && !matches!(count.who, ast::Side::Controller)
        {
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
        let selector = Selector::Zone {
            players: PlayerSet {
                relation: match count.who {
                    ast::Side::Controller => PlayerRelation::SelfPlayer,
                    ast::Side::Opponent => PlayerRelation::Opponents,
                    ast::Side::All => PlayerRelation::All,
                },
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(u64::from(!matches!(count.who, ast::Side::All))),
                    maximum: matches!(count.who, ast::Side::Controller | ast::Side::Opponent)
                        .then_some(Nat(1)),
                },
                text_quantifier: if matches!(count.who, ast::Side::All) {
                    TextQuantifier::All
                } else {
                    TextQuantifier::Implicit
                },
            },
            zone: match count.zone {
                ast::CountZone::Field => ZoneKind::Field,
                ast::CountZone::Ex => ZoneKind::Ex,
                ast::CountZone::EvolveDeck => ZoneKind::Evolve,
                ast::CountZone::Deck => ZoneKind::Deck,
                ast::CountZone::Hand => ZoneKind::Hand,
                ast::CountZone::Cemetery => ZoneKind::Cemetery,
                ast::CountZone::Banish => ZoneKind::Banish,
            },
            view: SelectionView::Current {},
            binding,
            predicate: Box::new(predicate),
        };
        let selector = if count.other {
            Selector::Difference {
                input: Box::new(selector),
                except: Box::new(super::numeric::holder()),
            }
        } else {
            selector
        };
        Ok(super::numeric::aggregate_count(selector, None))
    }
}

fn this_number(
    reference: &ast::This,
    property: Option<Stat>,
    view: CharacteristicView,
) -> Expr<Int> {
    let ast::This::This = reference;
    let object = ObjectView::SourceInformation {};
    let information = match property {
        Some(property) => NumberRead::Stats {
            object,
            property,
            view,
        },
        None => NumberRead::Cost { object, view },
    };
    Expr::Compute(Box::new(IntOperation::InformationOrZero { information }))
}

fn counter_number(counter: &ast::CounterRead, view: CounterReadView) -> Expr<Int> {
    let ast::This::This = counter.on;
    if matches!(view, CounterReadView::SourceInformation) {
        return Expr::Compute(Box::new(IntOperation::FromNat {
            value: Expr::Frozen(Box::new(NatFrozen::SnapshotCounter {
                object: Expr::Frozen(Box::new(SnapshotFields::SourceInformation {})),
                kind: CounterKind(counter.name.clone()),
            })),
        }));
    }
    Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Live(Box::new(NatRead::Counter {
            holder: super::counters::holder(),
            kind: CounterKind(counter.name.clone()),
        })),
    }))
}

fn turn_ordinal(who: ast::LeaderSide) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Live(Box::new(NatRead::TurnOrdinal {
            player: match who {
                ast::LeaderSide::Controller => PlayerRef::Controller {},
                ast::LeaderSide::Opponent => super::super::events::timing::opponent(),
            },
        })),
    }))
}
