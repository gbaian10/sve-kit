use super::super::{
    ChoiceDomain, Comparison, Control, CostSpec, DamageRecipient, Decision, DecisionId,
    DecisionKind, Disjointness, ExecutionRef, InputMode, MemberProjection, Mode, ModeChoiceKind,
    ModeDecision, ModeDecisionKind, ModeId, NatFrozen, NatOperation, ObjectProperty, ObjectReceipt,
    ObjectRef, PaymentOptionality, PaymentTime, PlaySlot, PlaySpec, Predicate, Program, ReceiptId,
    SelectionConstraint, SelectionGroup, SelectionSnapshot, SpellCountTime, Visibility,
};
use super::{
    AbilitySource, Action, Allocator, BTreeMap, BTreeSet, BatchPolicy, Binding, BodyContext, Bool,
    Cardinality, DamageAmount, DamageFlags, Expr, Int, Located, LoweringError, Nat, Node,
    ObjectView, PlayerRef, PlayerRelation, PlayerSet, RecipientSet, SelectionView, Selector,
    SourceSpan, TextQuantifier, ZoneKind, ast, unsupported,
};
use crate::ir::public_numbers;
use core::slice;

const fn parameter(id: u64, slot: PlaySlot, domain: ChoiceDomain) -> Decision {
    Decision {
        id: DecisionId(id),
        captures: vec![],
        actor: PlayerRef::Controller {},
        domain,
        kind: DecisionKind::PlayParameter {
            slot,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility: Visibility::Public {},
    }
}

fn target_zones(zone: &ast::TargetZone) -> Option<Vec<ZoneKind>> {
    match zone {
        ast::TargetZone::Single(single) => Some(vec![match single {
            ast::TargetSingleZone::Field => ZoneKind::Field,
            ast::TargetSingleZone::Ex => ZoneKind::Ex,
            ast::TargetSingleZone::Cemetery => ZoneKind::Cemetery,
        }]),
        ast::TargetZone::PublicPair(zones)
            if zones.len() == 2
                && zones.contains(&ast::TargetSingleZone::Field)
                && zones.contains(&ast::TargetSingleZone::Ex) =>
        {
            Some(vec![ZoneKind::Field, ZoneKind::Ex])
        }
        ast::TargetZone::PublicPair(_) => None,
    }
}

pub(super) fn trigger_play_spec(
    entry: &ast::Ability,
    body: &ast::Body,
    trigger_choice: bool,
) -> Option<PlaySpec> {
    let triggered = entry.activate.is_none() && entry.spell.is_none();
    (triggered
        && (body.optional_cost.is_some()
            || body.head_cost.is_some()
            || !body.targets.is_empty()
            || trigger_choice))
        .then(|| PlaySpec {
            limit: None,
            captures: vec![],
            parameters: vec![],
            costs: CostSpec::Ordered { steps: vec![] },
        })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Formal play shares the ability's binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn targets(
        &mut self,
        targets: &[ast::Target],
        choice: bool,
        path: &str,
    ) -> Result<(), LoweringError> {
        let mut names = BTreeSet::new();
        for target in targets {
            if self.definitions.contains_key(&target.bind) || !names.insert(&target.bind) {
                return Err(unsupported(path));
            }
            match (choice, target.option) {
                (true, Some(option)) => self
                    .mode_targets
                    .entry(option)
                    .or_default()
                    .push(target.clone()),
                (false, None) => self.target(target, path)?,
                _ => return Err(unsupported(path)),
            }
        }
        Ok(())
    }

    fn target(&mut self, target: &ast::Target, path: &str) -> Result<(), LoweringError> {
        let (domain, _) = self.object_choice(target, path, false)?;
        self.parameters.push(parameter(
            self.card_allocator.id(path)?,
            PlaySlot::Targets,
            domain,
        ));
        Ok(())
    }

    fn public_target_selector(
        &mut self,
        query: &ast::TargetQuery,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let zones = query
            .zone
            .as_ref()
            .and_then(target_zones)
            .ok_or_else(|| unsupported(path))?;
        if query.has_state_filter() && zones != [ZoneKind::Field] {
            return Err(unsupported(path));
        }
        let mut sets = Vec::new();
        for zone in zones {
            let candidate = Binding::new(1, self.card_allocator.scope(path)?);
            sets.push(Selector::Zone {
                players: PlayerSet {
                    relation: match query.who.as_ref().ok_or_else(|| unsupported(path))? {
                        ast::Side::Controller => PlayerRelation::SelfPlayer,
                        ast::Side::Opponent => PlayerRelation::Opponents,
                        ast::Side::All => PlayerRelation::All,
                    },
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: if matches!(query.who, Some(ast::Side::All)) {
                        TextQuantifier::All
                    } else {
                        TextQuantifier::Implicit
                    },
                },
                zone,
                view: SelectionView::Current {},
                binding: candidate,
                predicate: Box::new({
                    let predicate = super::selection::field_filter(
                        &super::selection::ObjectFilter::from(query),
                        candidate,
                    );
                    let predicate = self.numeric_limits(
                        predicate,
                        candidate,
                        query.cost_le.as_ref(),
                        query.orig_cost_le.as_ref(),
                        path,
                    )?;
                    if query.not_same_name_as_this {
                        super::search::exclude_holder_name(predicate, candidate)
                    } else {
                        predicate
                    }
                }),
            });
        }
        let mut selector = if sets.len() == 1 {
            sets.pop().ok_or_else(|| unsupported(path))?
        } else {
            Selector::Union { sets }
        };
        if query.other {
            selector = Selector::Difference {
                input: Box::new(selector),
                except: Box::new(super::numeric::holder()),
            };
        }
        Ok(selector)
    }

    pub(super) fn object_choice(
        &mut self,
        target: &ast::Target,
        path: &str,
        resolution: bool,
    ) -> Result<(ChoiceDomain, Cardinality), LoweringError> {
        let dynamic_count = target
            .n
            .as_ref()
            .or(target.upto.as_ref())
            .is_some_and(ast::NumericArgument::is_dynamic);
        if dynamic_count
            && (resolution
                || target.from.observation.is_some()
                || target.from.leader.is_some()
                || target.from.leaders.is_some()
                || target.from.leader_or)
        {
            return Err(unsupported(path));
        }
        if target.from.observation.is_some() {
            if target
                .from
                .cost_le
                .as_ref()
                .is_some_and(ast::NumericArgument::is_dynamic)
                || target
                    .from
                    .orig_cost_le
                    .as_ref()
                    .is_some_and(ast::NumericArgument::is_dynamic)
            {
                return Err(unsupported(path));
            }
            return self.observed_object_choice(
                target,
                slice::from_ref(&target.from),
                path,
                resolution,
            );
        }
        if target.from.not_same_name_as_this
            && (target.from.leader.is_some()
                || target.from.leaders.is_some()
                || target.from.leader_or)
        {
            return Err(unsupported(path));
        }
        if matches!(
            target.from.zone,
            Some(ast::TargetZone::Single(ast::TargetSingleZone::Cemetery))
        ) && (target.from.leader_or || !matches!(target.from.who, Some(ast::Side::Controller)))
        {
            return Err(unsupported(path));
        }
        let cardinality = target_cardinality(target, path)?;
        if target.from.leader.is_some() || target.from.leaders.is_some() || target.from.leader_or {
            if target.from.other {
                return Err(unsupported(path));
            }
            return self.recipient_choice(target, cardinality, path);
        }
        if self.recipient_targets.contains_key(&target.bind) {
            return Err(unsupported(path));
        }
        let binding = Binding::new(self.allocator.id(path)?, self.scope);
        if self.definitions.contains_key(&target.bind)
            || self.searches.contains(&target.bind)
            || self.observations.contains_key(&target.bind)
            || self.targets.insert(target.bind.clone(), binding).is_some()
        {
            return Err(unsupported(path));
        }
        if matches!(
            target.from.zone,
            Some(ast::TargetZone::Single(ast::TargetSingleZone::Ex))
        ) {
            self.ex_targets.insert(target.bind.clone());
        }
        let selector = self.public_target_selector(&target.from, path)?;
        let selector = self.target_extremum(&target.from, selector, path)?;
        let aggregate_bounds = if dynamic_count {
            vec![self.dynamic_target_count(target, path)?]
        } else {
            vec![]
        };
        Ok((
            ChoiceDomain::Objects {
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector,
                        cardinality,
                    }],
                    cardinality,
                    distinct_by: target.distinct_by.as_ref().map(|_| ObjectProperty::Names),
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds,
                },
                binding,
                target_roles: None,
            },
            cardinality,
        ))
    }

    fn dynamic_target_count(
        &mut self,
        target: &ast::Target,
        path: &str,
    ) -> Result<super::super::AggregateBound, LoweringError> {
        let Some(ast::NumericArgument::Expression(argument)) =
            target.n.as_ref().or(target.upto.as_ref())
        else {
            return Err(unsupported(path));
        };
        let number: ast::Number = super::decode(argument.as_ref().clone(), path)?;
        let value = self.number(&number, path, &mut BTreeSet::new())?;
        if !public_numbers::check(&value) {
            return Err(unsupported(path));
        }
        Ok(super::super::AggregateBound {
            property: super::super::NumericProperty::Count,
            comparison: if target.n.is_some() {
                Comparison::Equal
            } else {
                Comparison::LessEqual
            },
            value,
        })
    }

    /// Each branch filters the same Look receipt; several branches form one OR candidate set.
    pub(super) fn observed_object_choice(
        &mut self,
        target: &ast::Target,
        branches: &[ast::TargetQuery],
        path: &str,
        resolution: bool,
    ) -> Result<(ChoiceDomain, Cardinality), LoweringError> {
        let observation = target
            .from
            .observation
            .as_ref()
            .ok_or_else(|| unsupported(path))?;
        if !resolution
            || branches.iter().any(|branch| {
                branch.observation.as_ref() != Some(observation)
                    || branch.alias.is_some()
                    || branch.r#where.is_some()
                    || branch.who.is_some()
                    || branch.zone.is_some()
                    || branch.leader.is_some()
                    || branch.leaders.is_some()
                    || branch.leader_or
                    || branch.other
                    || branch.has_evolve.is_some()
                    || branch.has_state_filter()
                    || branch.hp_le.is_some()
            })
            || target.distinct_by.is_some()
            || self.recipient_targets.contains_key(&target.bind)
            || self.targets.contains_key(&target.bind)
            || self.definitions.contains_key(&target.bind)
            || self.searches.contains(&target.bind)
        {
            return Err(unsupported(path));
        }
        let receipt = self
            .observations
            .get(observation)
            .filter(|receipt| matches!(receipt, ObjectReceipt::Look { .. }))
            .ok_or_else(|| unsupported(path))?
            .clone();
        let cardinality = match (
            target.n.as_ref().and_then(ast::NumericArgument::literal),
            target.upto.as_ref().and_then(ast::NumericArgument::literal),
            target.any,
        ) {
            (Some(count), None, false) => Cardinality {
                minimum: Nat(count),
                maximum: Some(Nat(count)),
            },
            (None, Some(count), false) => Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(count)),
            },
            _ => return Err(unsupported(path)),
        };
        let binding = Binding::new(self.allocator.id(path)?, self.scope);
        if self.observations.contains_key(&target.bind)
            || self.targets.insert(target.bind.clone(), binding).is_some()
        {
            return Err(unsupported(path));
        }
        self.observed_targets.insert(target.bind.clone());
        if cardinality.maximum.is_none_or(|maximum| maximum.0 > 1) {
            self.field_entry.multi.insert(target.bind.clone());
        }
        let candidate = Binding::new(1, self.card_allocator.scope(path)?);
        let predicate = observed_predicate(branches, candidate).ok_or_else(|| unsupported(path))?;
        let selector = Selector::Filter {
            input: Box::new(Selector::ReceiptMembers {
                receipt,
                projection: MemberProjection::Before,
            }),
            binding: candidate,
            predicate: Box::new(predicate),
        };
        Ok((
            ChoiceDomain::Objects {
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector,
                        cardinality,
                    }],
                    cardinality,
                    distinct_by: None::<ObjectProperty>,
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds: vec![],
                },
                binding,
                target_roles: None,
            },
            cardinality,
        ))
    }

    fn recipient_choice(
        &mut self,
        target: &ast::Target,
        cardinality: Cardinality,
        path: &str,
    ) -> Result<(ChoiceDomain, Cardinality), LoweringError> {
        if target.distinct_by.is_some()
            || target.from.keyword.is_some()
            || target.from.r#trait.is_some()
            || !target.from.traits_all.is_empty()
            || target.from.token.is_some()
            || !target.from.names.is_empty()
            || target.from.has_evolve.is_some()
            || target.from.has_state_filter()
            || target.from.has_numeric_filter()
            || target.from.class.is_some()
            || target.from.not_class.is_some()
            || self.recipient_targets.contains_key(&target.bind)
            || self.targets.contains_key(&target.bind)
            || self.definitions.contains_key(&target.bind)
            || self.searches.contains(&target.bind)
            || self.observations.contains_key(&target.bind)
        {
            return Err(unsupported(path));
        }
        let players = recipient_players(&target.from, path)?;
        let leaders = RecipientSet::Leaders {
            players: players.clone(),
        };
        let candidates = if target.from.leader_or {
            let candidate = Binding::new(1, self.card_allocator.scope(path)?);
            RecipientSet::Union {
                sets: vec![
                    leaders,
                    RecipientSet::Objects {
                        objects: Selector::Zone {
                            players,
                            zone: ZoneKind::Field,
                            view: SelectionView::Current {},
                            binding: candidate,
                            predicate: Box::new(super::search::object_filter(
                                target.from.kind.as_ref(),
                                None,
                                None,
                                candidate,
                            )),
                        },
                    },
                ],
            }
        } else {
            leaders
        };
        let binding = Binding::new(self.allocator.id(path)?, self.scope);
        self.recipient_targets.insert(target.bind.clone(), binding);
        Ok((
            ChoiceDomain::Recipients {
                candidates,
                cardinality,
                binding,
            },
            cardinality,
        ))
    }

    pub(super) fn recipients(
        &mut self,
        recipients: &ast::Recipients,
        path: &str,
    ) -> Result<RecipientSet, LoweringError> {
        Ok(match recipients {
            ast::Recipients::Field(query) => RecipientSet::Objects {
                objects: self.objects(&ast::Objects::Zone(query.clone()), path)?,
            },
            ast::Recipients::Leader { leader } => RecipientSet::Explicit {
                recipients: vec![DamageRecipient::Leader {
                    player: match leader {
                        ast::LeaderSide::Controller => PlayerRef::Controller {},
                        ast::LeaderSide::Opponent => super::super::events::timing::opponent(),
                    },
                }],
            },
            ast::Recipients::Leaders { leaders } => RecipientSet::Leaders {
                players: PlayerSet {
                    relation: match leaders {
                        ast::LeaderGroup::All => PlayerRelation::All,
                        ast::LeaderGroup::Opponents => PlayerRelation::Opponents,
                    },
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: TextQuantifier::All,
                },
            },
            ast::Recipients::Target(name) if self.recipient_targets.contains_key(name) => {
                RecipientSet::Bound {
                    recipients: *self
                        .recipient_targets
                        .get(name)
                        .ok_or_else(|| unsupported(path))?,
                }
            }
            ast::Recipients::LeaderOf { leader_of } => RecipientSet::Explicit {
                recipients: vec![DamageRecipient::Leader {
                    player: self.selected_player(
                        leader_of,
                        super::super::Role::Controller,
                        path,
                    )?,
                }],
            },
            ast::Recipients::Target(name) => RecipientSet::Objects {
                objects: Selector::Bound {
                    reference: *self.targets.get(name).ok_or_else(|| unsupported(path))?,
                },
            },
            ast::Recipients::Union { any_of } => RecipientSet::Union {
                sets: any_of
                    .iter()
                    .map(|set| self.recipients(set, path))
                    .collect::<Result<Vec<_>, _>>()?,
            },
        })
    }

    pub(super) fn damage(
        &mut self,
        damage: &ast::Damage,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let recipients = self.recipients(&damage.to, path)?;
        let mut value = self.number(&damage.n, path, &mut BTreeSet::new())?;
        if let Some(replacements) = &damage.instead {
            let [replacement] = replacements.as_slice() else {
                return Err(unsupported(path));
            };
            value = Expr::Compute(Box::new(super::IntOperation::Conditional {
                condition: self.condition(&replacement.r#if, path)?,
                then_value: self.number(&replacement.n, path, &mut BTreeSet::new())?,
                else_value: value,
            }));
        }
        let amount = if damage.distribute == Some(true) {
            if !self.formal_play
                || self.targets.len() != 1
                || self.parameters.len() != 1
                || !matches!(&damage.to, ast::Recipients::Target(_))
            {
                return Err(unsupported(path));
            }
            let total = if let Expr::Literal(Int(total)) = value {
                Expr::Literal(Nat(
                    u64::try_from(total).map_err(|_private| unsupported(path))?
                ))
            } else {
                Expr::Compute(Box::new(NatOperation::CheckedInt { value }))
            };
            let binding = Binding::new(self.allocator.id(path)?, self.scope);
            self.parameters.push(parameter(
                self.card_allocator.id(path)?,
                PlaySlot::Distribution,
                ChoiceDomain::Distribution {
                    recipients: recipients.clone(),
                    total,
                    minimum: Nat(1),
                    binding,
                },
            ));
            DamageAmount::Allocation { binding }
        } else {
            if damage.distribute_at.is_some() {
                return Err(unsupported(path));
            }
            DamageAmount::Uniform { amount: value }
        };
        Ok(Action::Damage {
            recipients,
            amount,
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        })
    }

    pub(super) fn choice(
        &mut self,
        choice: &ast::Choice,
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        if !self.formal_play || choice.options.is_empty() {
            return Err(unsupported(path));
        }
        let maximum = u64::try_from(choice.options.len()).map_err(|_private| unsupported(path))?;
        let (count, cardinality) = if let Some(ast::ModeCap::Count { count }) = &choice.upto {
            if choice.n.is_some() || choice.instead.is_some() || choice.exclude_chosen.is_some() {
                return Err(unsupported(path));
            }
            let ast::CountQuery {
                face_up: None,
                class: None,
                not_class: None,
                other: false,
                who: ast::Side::Controller,
                zone: ast::CountZone::Field,
                kind: None,
                keyword: None,
                r#trait: Some(_),
                name: None,
                name_contains: None,
                orig_cost_eq: None,
                orig_cost_ge: None,
                token: None,
                not_token: None,
            } = count
            else {
                return Err(unsupported(path));
            };
            let count_expression = self.zone_count(count, path)?;
            let Expr::Compute(operation) = count_expression else {
                return Err(unsupported(path));
            };
            let super::super::IntOperation::FromNat { value } = *operation else {
                return Err(unsupported(path));
            };
            (
                value,
                Cardinality {
                    minimum: Nat(1),
                    maximum: None,
                },
            )
        } else if let Some(ast::ModeCap::Number(argument)) = &choice.upto {
            if choice.n.is_some() || choice.instead.is_some() || choice.exclude_chosen.is_some() {
                return Err(unsupported(path));
            }
            let number: ast::Number = super::decode(argument.as_ref().clone(), path)?;
            if !matches!(&number,ast::Number::Per{per} if per.unit>0 && matches!(per.of,ast::Number::Count{..}))
            {
                return Err(unsupported(path));
            }
            let value = self.number(&number, path, &mut BTreeSet::new())?;
            if !public_numbers::check(&value) {
                return Err(unsupported(path));
            }
            (
                Expr::Compute(Box::new(NatOperation::CheckedInt { value })),
                Cardinality {
                    minimum: Nat(1),
                    maximum: None,
                },
            )
        } else {
            choice_count(self, choice, maximum, path)?
        };
        if choice.exclude_chosen.is_some()
            && (choice.n != Some(1) || choice.upto.is_some() || choice.instead.is_some())
        {
            return Err(unsupported(path));
        }
        if self.mode_targets.keys().any(|id| *id == 0 || *id > maximum) {
            return Err(unsupported(path));
        }
        self.modes(
            ModeShape {
                options: &choice.options,
                count,
                cardinality,
                exclude: choice.exclude_chosen.is_some(),
                kind: ModeChoiceKind::ChoiceKeyword,
            },
            path,
            source,
            ability_source,
        )
    }

    pub(super) fn either(
        &mut self,
        either: &ast::Either,
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        if either.options.len() < 2 || either.bind.is_some() {
            return Err(unsupported(path));
        }
        self.modes(
            ModeShape {
                options: &either.options,
                count: Expr::Literal(Nat(1)),
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                exclude: false,
                kind: ModeChoiceKind::ResolutionAlternative,
            },
            path,
            source,
            ability_source,
        )
    }

    fn modes(
        &mut self,
        shape: ModeShape<'_>,
        path: &str,
        source: &SourceSpan,
        ability_source: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        let resolution = shape.kind == ModeChoiceKind::ResolutionAlternative;
        let mut modes = Vec::new();
        for (index, steps) in shape.options.iter().enumerate() {
            let mode_id = index.checked_add(1).ok_or_else(|| unsupported(path))?;
            let mode_id = u64::try_from(mode_id).map_err(|_error| unsupported(path))?;
            let scope = self.card_allocator.scope(path)?;
            let mut context = BodyContext {
                subject_event: self.subject_event,
                damage_event: self.damage_event,
                source_use: self.source_use,
                counter_read_view: self.counter_read_view,
                players: self.players.clone(),
                leave_event: self.leave_event,
                entry_event: self.entry_event,
                evolution: None,
                source_can_enter_field: self.source_can_enter_field,
                departure_to_cemetery: self.departure_to_cemetery,
                scope,
                definition: self.definition,
                payment_result: None,
                local_payment: self.local_payment.clone(),
                allocator: Allocator {
                    next: self.allocator.next,
                    ..Allocator::default()
                },
                card_allocator: self.card_allocator,
                definitions: self.definitions,
                searches: BTreeSet::new(),
                ex_outputs: BTreeMap::new(),
                ex_targets: BTreeSet::new(),
                observations: BTreeMap::new(),
                single_discards: BTreeSet::new(),
                observed_targets: BTreeSet::new(),
                field_entry: super::FieldEntryGuard::default(),
                targets: BTreeMap::new(),
                recipient_targets: BTreeMap::new(),
                parameters: vec![],
                resolution_scope: self.resolution_scope,
                resolution_captures: vec![],
                formal_play: false,
                auto: None,
                mode_targets: BTreeMap::new(),
            };
            if resolution {
                context.payment_result.clone_from(&self.payment_result);
                context.searches.clone_from(&self.searches);
                context.ex_outputs.clone_from(&self.ex_outputs);
                context.ex_targets.clone_from(&self.ex_targets);
                context.observations.clone_from(&self.observations);
                context.single_discards.clone_from(&self.single_discards);
                context.observed_targets.clone_from(&self.observed_targets);
                context.field_entry.multi = self.field_entry.multi.clone();
                context.targets.clone_from(&self.targets);
                context
                    .recipient_targets
                    .clone_from(&self.recipient_targets);
            }
            if !resolution && let Some(targets) = self.mode_targets.get(&mode_id) {
                for target in targets {
                    context.target(target, path)?;
                }
            }
            let head = if resolution { "either" } else { "choice" };
            let body = mode_body(
                &mut context,
                steps,
                resolution,
                source,
                ability_source,
                &format!("{path}/{head}/options/{index}"),
            )?;
            self.allocator.next = context.allocator.next;
            self.resolution_captures
                .append(&mut context.resolution_captures);
            modes.push(Mode {
                costs: CostSpec::Ordered { steps: vec![] },
                id: ModeId(mode_id),
                parameters: context.parameters,
                body,
            });
        }
        let id = self.card_allocator.id(path)?;
        let chosen = shape.decision(
            id,
            Binding::new(self.allocator.id(path)?, self.scope),
            &modes,
        );
        let mut source = source.clone();
        path.clone_into(&mut source.author_path);
        Ok(Located {
            source,
            node: Node::Control {
                control: Control::Mode {
                    modes,
                    count: shape.count,
                    chosen,
                    choice_kind: shape.kind,
                },
            },
        })
    }
}

fn mode_body(
    context: &mut BodyContext<'_>,
    steps: &[ast::Step],
    resolution: bool,
    source: &SourceSpan,
    origin: &AbilitySource,
    path: &str,
) -> Result<Program, LoweringError> {
    let [ast::Step::Pay(payment)] = steps else {
        return context.steps(steps, path, source, origin);
    };
    if resolution {
        return Err(unsupported(path));
    }
    let paid = super::payment::lower(context, &payment.cost, origin, path)?;
    context.parameters.extend(paid.parameters);
    let scope = context.scope;
    context.scope = context.card_allocator.scope(path)?;
    let body = context.steps(&payment.steps, path, source, origin)?;
    context.scope = scope;
    let receipt = ReceiptId(context.card_allocator.id(path)?);
    Ok(Program {
        scope,
        steps: vec![BodyContext::located_step(
            source,
            path,
            Node::Control {
                control: Control::Pay {
                    at: PaymentTime::PlayParameter,
                    costs: paid.costs,
                    optional: PaymentOptionality::Optional,
                    on_paid: body,
                    receipt,
                },
            },
        )],
    })
}

struct ModeShape<'options> {
    options: &'options [Vec<ast::Step>],
    count: Expr<Nat>,
    cardinality: Cardinality,
    exclude: bool,
    kind: ModeChoiceKind,
}

impl ModeShape<'_> {
    fn decision(&self, id: u64, binding: Binding<Vec<ModeId>>, modes: &[Mode]) -> ModeDecision {
        ModeDecision {
            id: DecisionId(id),
            actor: PlayerRef::Controller {},
            domain: ChoiceDomain::Modes {
                modes: modes.iter().map(|mode| mode.id).collect(),
                cardinality: self.cardinality,
                binding,
                exclude_used: self
                    .exclude
                    .then_some(super::super::ModeExclusion::CurrentTurnAbilityInstance),
            },
            kind: if self.kind == ModeChoiceKind::ResolutionAlternative {
                ModeDecisionKind::ResolutionChoice {
                    cardinality: self.cardinality,
                }
            } else {
                ModeDecisionKind::PlayParameter {
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                }
            },
            visibility: Visibility::Public {},
        }
    }
}

fn choice_count(
    context: &mut BodyContext<'_>,
    choice: &ast::Choice,
    printed: u64,
    path: &str,
) -> Result<(Expr<Nat>, Cardinality), LoweringError> {
    let upto = match &choice.upto {
        None => None,
        Some(ast::ModeCap::Fixed(number)) => Some(*number),
        Some(ast::ModeCap::Count { .. } | ast::ModeCap::Number(_)) => {
            return Err(unsupported(path));
        }
    };
    let (number, exact) = quantity(choice.n, upto, path)?;
    let cap = number.min(printed);
    let bounds = Cardinality {
        minimum: Nat(if exact { cap } else { u64::from(cap > 0) }),
        maximum: Some(Nat(cap)),
    };
    let Some(replacements) = &choice.instead else {
        return Ok((Expr::Literal(Nat(number)), bounds));
    };
    // Multiple matching replacements require an independently specified precedence.
    let [replacement] = replacements.as_slice() else {
        return Err(unsupported(path));
    };
    let (changed, changed_exact) = quantity(replacement.n, replacement.upto, path)?;
    if (exact && number != 1) || (changed_exact && changed != 1) {
        return Err(unsupported(path));
    }
    let at_least = |read, threshold| {
        Expr::Compute(Box::new(Predicate::CompareNat {
            left: Expr::Frozen(Box::new(read)),
            comparison: Comparison::GreaterEqual,
            right: Expr::Literal(Nat(threshold)),
        }))
    };
    let condition = match &replacement.r#if {
        ast::ModeCondition::Cmp(comparison) => {
            context.public_evolve_mode_condition(comparison, path)?
        }
        ast::ModeCondition::EnteredFrom(origin)
            if matches!(origin.zone, ast::EntryOriginZone::Cemetery) =>
        {
            context.entered_from(origin, path)?
        }
        ast::ModeCondition::EnteredFrom(_) => return Err(unsupported(path)),
        ast::ModeCondition::Spellchain(threshold) => at_least(
            NatFrozen::SpellChain {
                player: PlayerRef::Controller {},
                execution: ExecutionRef::Current {},
                at: SpellCountTime::AtPlay,
            },
            *threshold,
        ),
        ast::ModeCondition::Necro(threshold) => at_least(
            NatFrozen::NecroCharge {
                player: PlayerRef::Controller {},
                execution: ExecutionRef::Current {},
                at: SpellCountTime::AtPlay,
            },
            *threshold,
        ),
    };
    Ok((
        Expr::Compute(Box::new(NatOperation::Conditional {
            condition,
            then_value: Expr::Literal(Nat(changed)),
            else_value: Expr::Literal(Nat(number)),
        })),
        Cardinality {
            minimum: Nat(1),
            maximum: None,
        },
    ))
}

fn quantity(
    exact: Option<u64>,
    upto: Option<u64>,
    path: &str,
) -> Result<(u64, bool), LoweringError> {
    match (exact, upto) {
        (Some(n), None) => Ok((n, true)),
        (None, Some(n)) => Ok((n, false)),
        _ => Err(unsupported(path)),
    }
}

/// Only one top-level mode group is admitted; other instructions stay in the body.
pub(super) fn choice_body(body: &ast::Body, path: &str) -> Result<bool, LoweringError> {
    if body.steps.iter().any(|step| matches!(step, ast::Step::If(branch)
        if contains_choice(&branch.then) || branch.r#else.as_ref().is_some_and(|continuation| contains_choice(continuation)))) {
        return Err(unsupported(path));
    }
    match body
        .steps
        .iter()
        .filter(|step| matches!(step, ast::Step::Choice(_)))
        .count()
    {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(unsupported(path)),
    }
}

fn contains_choice(steps: &[ast::Step]) -> bool {
    steps.iter().any(|step| match step {
        ast::Step::Choice(_) => true,
        ast::Step::Pay(payment) => contains_choice(&payment.steps),
        ast::Step::Optional(optional) => {
            contains_choice(&optional.steps)
                || optional
                    .then
                    .as_ref()
                    .is_some_and(|continuation| contains_choice(continuation))
        }
        ast::Step::EachPlayer(each) => each.stages.iter().any(|stage| contains_choice(stage)),
        ast::Step::If(branch) => {
            contains_choice(&branch.then)
                || branch
                    .r#else
                    .as_ref()
                    .is_some_and(|else_steps| contains_choice(else_steps))
        }
        ast::Step::StackPlus(_)
        | ast::Step::Counter(_)
        | ast::Step::GainDrive(_)
        | ast::Step::Race(_)
        | ast::Step::Evolve(_)
        | ast::Step::Either(_)
        | ast::Step::Choose(_)
        | ast::Step::Draw(_)
        | ast::Step::Pp(_)
        | ast::Step::Discard(_)
        | ast::Step::PpMax(_)
        | ast::Step::Ep(_)
        | ast::Step::Damage(_)
        | ast::Step::Life(_)
        | ast::Step::SetLife(_)
        | ast::Step::Destroy(_)
        | ast::Step::Banish(_)
        | ast::Step::Transform(_)
        | ast::Step::Act(_)
        | ast::Step::Stand(_)
        | ast::Step::Buff(_)
        | ast::Step::Grant(_)
        | ast::Step::SetStat(_)
        | ast::Step::Apply(_)
        | ast::Step::PlayerEffect(_)
        | ast::Step::Search(_)
        | ast::Step::Look(_)
        | ast::Step::Reveal(_)
        | ast::Step::Move(_)
        | ast::Step::Create(_) => false,
    })
}

pub(super) fn trigger_choice(
    body: &ast::Body,
    triggered: bool,
    path: &str,
) -> Result<bool, LoweringError> {
    if !triggered || !contains_choice(&body.steps) {
        return Ok(false);
    }
    match body.steps.as_slice() {
        [ast::Step::Choice(_)] => Ok(true),
        [ast::Step::If(branch)]
            if (matches!(
                branch.cond,
                ast::Condition::Awakened(true) | ast::Condition::EnteredFrom(_)
            ) || matches!(&branch.cond, ast::Condition::Not(inner) if matches!(inner.as_ref(), ast::Condition::EnteredFrom(_))))
                && branch.r#else.is_none()
                && matches!(branch.then.as_slice(), [ast::Step::Choice(_)]) =>
        {
            Ok(true)
        }
        _ => Err(unsupported(path)),
    }
}

const fn target_players(side: &ast::Side) -> PlayerSet {
    PlayerSet {
        relative_to: PlayerRef::Controller {},
        relation: match side {
            ast::Side::Controller => PlayerRelation::SelfPlayer,
            ast::Side::Opponent => PlayerRelation::Opponents,
            ast::Side::All => PlayerRelation::All,
        },
        cardinality: Cardinality {
            minimum: Nat(0),
            maximum: None,
        },
        text_quantifier: TextQuantifier::Implicit,
    }
}

fn recipient_players(query: &ast::TargetQuery, path: &str) -> Result<PlayerSet, LoweringError> {
    let players = if let Some(leader) = query.leader {
        if query.has_evolve.is_some()
            || query.who.is_some()
            || query.zone.is_some()
            || query.kind.is_some()
            || query.leader_or
            || query.leaders.is_some()
        {
            return Err(unsupported(path));
        }
        PlayerSet {
            relative_to: PlayerRef::Controller {},
            relation: match leader {
                ast::LeaderSide::Controller => PlayerRelation::SelfPlayer,
                ast::LeaderSide::Opponent => PlayerRelation::Opponents,
            },
            cardinality: Cardinality {
                minimum: Nat(0),
                maximum: None,
            },
            text_quantifier: TextQuantifier::Implicit,
        }
    } else if let Some(leaders) = &query.leaders {
        if query.has_evolve.is_some()
            || query.who.is_some()
            || query.zone.is_some()
            || query.kind.is_some()
            || query.leader_or
        {
            return Err(unsupported(path));
        }
        target_players(match leaders {
            ast::LeaderGroup::All => &ast::Side::All,
            ast::LeaderGroup::Opponents => &ast::Side::Opponent,
        })
    } else {
        if !matches!(query.kind, Some(ast::SearchKind::Follower)) {
            return Err(unsupported(path));
        }
        target_players(query.who.as_ref().ok_or_else(|| unsupported(path))?)
    };
    Ok(players)
}

fn target_cardinality(target: &ast::Target, path: &str) -> Result<Cardinality, LoweringError> {
    if target.n.is_some() && target.upto.is_some() {
        return Err(unsupported(path));
    }
    if let Some(ast::NumericArgument::Expression(_)) = target.n.as_ref().or(target.upto.as_ref()) {
        if target.any {
            return Err(unsupported(path));
        }
        return Ok(Cardinality {
            minimum: Nat(0),
            maximum: None,
        });
    }
    match (
        target.n.as_ref().and_then(ast::NumericArgument::literal),
        target.upto.as_ref().and_then(ast::NumericArgument::literal),
        target.any,
    ) {
        (Some(n), None, false) => Ok(Cardinality {
            minimum: Nat(n),
            maximum: Some(Nat(n)),
        }),
        (None, Some(n), false) => Ok(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(n)),
        }),
        (None, None, true) => Ok(Cardinality {
            minimum: Nat(0),
            maximum: None,
        }),
        _ => Err(unsupported(path)),
    }
}

/// One branch keeps its filter; a union ORs filtered branches of the same Look.
fn observed_predicate(
    branches: &[ast::TargetQuery],
    candidate: Binding<ObjectRef>,
) -> Option<Expr<Bool>> {
    if branches.iter().any(|q| {
        q.cost_le
            .as_ref()
            .is_some_and(ast::NumericArgument::is_dynamic)
            || q.orig_cost_le
                .as_ref()
                .is_some_and(ast::NumericArgument::is_dynamic)
    }) {
        return None;
    }
    let mut predicates = branches
        .iter()
        .map(|branch| {
            let predicate = super::selection::field_filter(
                &super::selection::ObjectFilter::from(branch),
                candidate,
            );
            if branch.not_same_name_as_this {
                super::search::exclude_source_name(predicate, candidate)
            } else {
                predicate
            }
        })
        .collect::<Vec<_>>();
    if predicates.len() == 1 {
        return predicates.pop();
    }
    // An unfiltered branch would make the union a plain choice from the whole Look.
    if predicates
        .iter()
        .any(|predicate| matches!(predicate, Expr::Literal(_)))
    {
        return None;
    }
    Some(Expr::Compute(Box::new(Predicate::Or { predicates })))
}
