use super::super::{
    CharacteristicPatch, ContinuousLayer, Duration, DurationEnd, Effect, EffectDefinitionId,
    EffectReference, EffectTarget, ExecutionRef, InstallationSubjects, InstallationTime, Interval,
    NumericChange, ObjectRef, ObjectView, RemoveScope, Resource, ResourceOperation, Stat,
    TimeAnchor, TimestampBasis, TraitId,
};
use super::{
    AbilitySource, Action, BTreeSet, Binding, BodyContext, Cardinality, Expr, Int, LoweringError,
    Nat, ObjectOperand, PlayerRef, PlayerRelation, PlayerSet, SelectionView, Selector,
    TextQuantifier, ZoneKind, ast, unsupported,
};
use crate::ir::ObjectRelation;

pub(super) fn holder() -> Selector {
    Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    }
}

pub(super) fn aggregate_count(
    selector: Selector,
    property: Option<super::super::ObjectProperty>,
) -> Expr<Int> {
    let value = match property {
        Some(property) => Expr::Compute(Box::new(super::super::NatOperation::DistinctCount {
            objects: selector,
            property,
        })),
        None => Expr::Live(Box::new(super::NatRead::ZoneCount { selector })),
    };
    Expr::Compute(Box::new(super::IntOperation::FromNat { value }))
}

pub(super) fn distinct_count(
    query: &ast::PublicQuery,
    property: super::super::ObjectProperty,
    binding: Binding<ObjectRef>,
    path: &str,
) -> Result<Expr<Int>, LoweringError> {
    if query.other {
        return Err(unsupported(path));
    }
    Ok(aggregate_count(
        super::costs::public_query(query, binding, path)?,
        Some(property),
    ))
}

pub(super) fn resource_increase(
    value: &ast::ResourceIncrease,
    resource: Resource,
    path: &str,
) -> Result<Action, LoweringError> {
    let ast::SelfPlayer::Controller = value.who;
    if value.delta == 0 {
        return Err(unsupported(path));
    }
    Ok(Action::Resource {
        player: PlayerRef::Controller {},
        resource,
        operation: ResourceOperation::Add,
        amount: Expr::Literal(Int(
            i64::try_from(value.delta).map_err(|_private| unsupported(path))?
        )),
    })
}

pub(super) fn duration(until: Option<&ast::Until>, path: &str) -> Result<Duration, LoweringError> {
    Ok(Duration {
        end: match until {
            Some(ast::Until::NextPhase { .. } | ast::Until::DuringNextTurn { .. }) => {
                return Err(unsupported(path));
            }
            None | Some(ast::Until::Game) => DurationEnd::NoTemporalExpiry {},
            Some(ast::Until::Turn | ast::Until::ThisAndNextOppTurn) => {
                DurationEnd::CurrentTurnEnd {}
            }
            Some(ast::Until::NextOppTurn) => DurationEnd::NextTurnEnd {
                player: super::super::events::timing::opponent(),
            },
        },
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: if matches!(until, Some(ast::Until::ThisAndNextOppTurn)) {
            vec![Interval::DuringNextTurn {
                player: super::super::events::timing::opponent(),
            }]
        } else {
            vec![]
        },
    })
}

fn combined(
    mut patches: Vec<CharacteristicPatch>,
    path: &str,
) -> Result<CharacteristicPatch, LoweringError> {
    match patches.len() {
        0 => Err(unsupported(path)),
        1 => patches.pop().ok_or_else(|| unsupported(path)),
        _ => Ok(CharacteristicPatch::Combined { patches }),
    }
}

impl BodyContext<'_> {
    /// 「それのリーダー」 follows the chosen object's controller, captured when it was chosen.
    pub(super) fn life(&mut self, life: &ast::Life, path: &str) -> Result<Action, LoweringError> {
        let player = match &life.who {
            ast::LifeHolder::Controller(ast::SelfPlayer::Controller) => PlayerRef::Controller {},
            ast::LifeHolder::MasterOf { master_of } => {
                self.selected_player(master_of, super::super::Role::Controller, path)?
            }
        };
        Ok(Action::Life {
            player,
            change: NumericChange::Add {
                amount: self.number(&life.delta, path, &mut BTreeSet::new())?,
            },
        })
    }

    pub(super) fn draw(&mut self, draw: &ast::Draw, path: &str) -> Result<Action, LoweringError> {
        let player = match &draw.who {
            None | Some(ast::LifeHolder::Controller(ast::SelfPlayer::Controller)) => {
                PlayerRef::Controller {}
            }
            Some(ast::LifeHolder::MasterOf { master_of }) => {
                self.selected_player(master_of, super::super::Role::Controller, path)?
            }
        };
        Ok(Action::Draw {
            player,
            count: self.number(&draw.n, path, &mut BTreeSet::new())?,
            mode: super::super::StepwiseMode::Exact,
        })
    }

    pub(super) fn set_life(
        &mut self,
        life: &ast::SetLife,
        path: &str,
    ) -> Result<Action, LoweringError> {
        Ok(Action::Life {
            player: match life.who {
                ast::SetLifeHolder::Controller => PlayerRef::Controller {},
                ast::SetLifeHolder::Opponents => super::super::events::timing::opponent(),
            },
            change: NumericChange::Set {
                amount: self.number(&life.n, path, &mut BTreeSet::new())?,
            },
        })
    }

    pub(super) fn objects(
        &mut self,
        value: &ast::Objects,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let query = match value {
            ast::Objects::This(_) => return Ok(holder()),
            ast::Objects::EventHolder {
                event: ast::HolderEvent::Holder,
            } => {
                return self
                    .equip_event
                    .map(|event| Selector::Objects {
                        objects: vec![ObjectOperand::FromExpression {
                            value: Box::new(super::super::events::equipment::holder(event)),
                        }],
                    })
                    .ok_or_else(|| unsupported(path));
            }
            ast::Objects::Bound(name) => {
                if name == "host" && self.static_zone == Some(ZoneKind::Equipment) {
                    return Ok(Selector::Relation {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::SourceHolder {}),
                        },
                        relation: ObjectRelation::Equipment,
                    });
                }
                if name == "attack_target" {
                    return self.attack_target_objects(path);
                }
                if name == "it" {
                    if let Some(event) = self.drive_checked_event {
                        return Ok(Selector::Objects {
                            objects: vec![ObjectOperand::FromExpression {
                                value: Box::new(super::super::events::drive::holder(event)),
                            }],
                        });
                    }
                    return self
                        .subject_event
                        .map(|event| Selector::Objects {
                            objects: vec![ObjectOperand::FromExpression {
                                value: Box::new(event.object()),
                            }],
                        })
                        .ok_or_else(|| unsupported(path));
                }
                return self
                    .targets
                    .get(name)
                    .copied()
                    .map(|reference| Selector::Bound { reference })
                    .ok_or_else(|| unsupported(path));
            }
            ast::Objects::Follow { follow } => {
                return self
                    .ex_outputs
                    .get(follow)
                    .cloned()
                    .ok_or_else(|| unsupported(path));
            }
            ast::Objects::Zone(query) => query,
        };
        match query.zone {
            ast::Zone::Field => self.zone_objects(query, ZoneKind::Field, path),
            ast::Zone::Ex => Err(unsupported(path)),
        }
    }

    pub(super) fn destroy_objects(
        &mut self,
        objects: &ast::Objects,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        if let ast::Objects::Zone(query) = objects
            && matches!(query.r#where, Some(ast::ObjectQueryWhere::Damaged(_)))
        {
            // Exhaustive so a new query field cannot silently widen this history shortcut.
            let ast::ObjectQuery {
                alias: None,
                r#where:
                    Some(ast::ObjectQueryWhere::Damaged(ast::DamagedThisTurnWhere {
                        history:
                            ast::DamagedThisTurnHistory {
                                event: ast::DamagedHistoryEvent::Damaged,
                                window: ast::ThisTurn::ThisTurn,
                                r#where:
                                    ast::DamagedThisTurnRecipient {
                                        recipient: ast::Item::Item,
                                    },
                            },
                    })),
                who: ast::Side::Opponent,
                zone: ast::Zone::Field,
                kind: Some(ast::FieldKind::Follower),
                r#trait: None,
                class: None,
                not_class: None,
                token: None,
                not_token: None,
                orig_cost_eq: None,
                orig_cost_le: None,
                hp_le: None,
                not_trait: None,
                name: None,
                name_contains: None,
                acted: None,
                except: None,
                other: false,
            } = &**query
            else {
                return Err(unsupported(path));
            };
            let candidate = Binding::new(self.allocator.id(path)?, self.scope);
            let event = Binding::new(self.allocator.id(&format!("{path}/damaged"))?, self.scope);
            return Ok(super::super::history_turn::public::damaged_selector(
                candidate, event,
            ));
        }
        self.objects(objects, path)
    }

    pub(super) fn zone_objects(
        &mut self,
        query: &ast::ObjectQuery,
        zone: ZoneKind,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        // Extremum populations do not carry these filters; reject them rather than widen.
        if (query.acted.is_some()
            || query.except.is_some()
            || query.hp_le.is_some()
            || query.not_trait.is_some())
            && (query.alias.is_some() || query.r#where.is_some())
        {
            return Err(unsupported(path));
        }
        let binding: Binding<ObjectRef> = Binding::new(self.allocator.id(path)?, self.scope);
        let kind = query.kind.map(ast::FieldKind::search);
        let objects = Selector::Zone {
            players: PlayerSet {
                relation: match query.who {
                    ast::Side::Controller => PlayerRelation::SelfPlayer,
                    ast::Side::Opponent => PlayerRelation::Opponents,
                    ast::Side::All => PlayerRelation::All,
                },
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                },
                text_quantifier: if matches!(query.who, ast::Side::All) {
                    TextQuantifier::All
                } else {
                    TextQuantifier::Implicit
                },
            },
            zone,
            view: SelectionView::Current {},
            binding,
            predicate: Box::new(super::selection::field_filter(
                &super::selection::ObjectFilter {
                    kind: kind.as_ref().map(super::search::card_kind),
                    trait_id: query.r#trait.as_deref(),
                    class: query.class.as_deref(),
                    not_class: query.not_class.as_deref(),
                    token: query
                        .token
                        .or_else(|| query.not_token.map(|excluded| !excluded)),
                    original_cost: [query.orig_cost_eq, query.orig_cost_le, None],
                    hp_at_most: query.hp_le,
                    not_trait: query.not_trait.as_deref(),
                    name: query.name.as_deref(),
                    contains: query.name_contains.as_deref(),
                    acted: query.acted,
                    ..super::selection::ObjectFilter::default()
                },
                binding,
            )),
        };
        let objects = if query.other {
            Selector::Difference {
                input: Box::new(objects),
                except: Box::new(holder()),
            }
        } else {
            objects
        };
        let objects = match &query.except {
            Some(except) => {
                // Only a formal or resolution target chosen earlier in this ability qualifies.
                let reference = self
                    .targets
                    .get(&except.reference)
                    .copied()
                    .filter(|_| !self.observed_targets.contains(&except.reference))
                    .ok_or_else(|| unsupported(path))?;
                Selector::Difference {
                    input: Box::new(objects),
                    except: Box::new(Selector::Bound { reference }),
                }
            }
            None => objects,
        };
        self.extremum_query(query, objects, path)
    }

    pub(super) fn action_objects(
        &mut self,
        action: &ast::ObjectAction,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        if matches!(
            action,
            ast::ObjectAction::Long {
                result: Some(_),
                ..
            }
        ) {
            return Err(unsupported(path));
        }
        match action {
            ast::ObjectAction::Short(objects) | ast::ObjectAction::Long { r#ref: objects, .. } => {
                self.objects(objects, path)
            }
        }
    }

    pub(super) fn modify(
        &mut self,
        modification: &ast::Modify,
        set: bool,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if modification.instead.is_some()
            && (set || modification.power.is_none() || modification.hp.is_some())
        {
            return Err(unsupported(path));
        }
        let mut patches = Vec::new();
        for (stat, value) in [
            (Stat::Attack, &modification.power),
            (Stat::Life, &modification.hp),
        ] {
            if let Some(value) = value {
                let mut amount = self.number(value, path, &mut BTreeSet::new())?;
                if let Some(replacements) = &modification.instead {
                    let [replacement] = replacements.as_slice() else {
                        return Err(unsupported(path));
                    };
                    amount = Expr::Compute(Box::new(super::IntOperation::Conditional {
                        condition: self.condition(&replacement.r#if, path)?,
                        then_value: self.number(&replacement.power, path, &mut BTreeSet::new())?,
                        else_value: amount,
                    }));
                }
                if set && !matches!(&amount, Expr::Literal(Int(literal)) if *literal >= 0) {
                    return Err(unsupported(path));
                }
                patches.push(CharacteristicPatch::Numeric {
                    stat,
                    change: if set {
                        NumericChange::Set { amount }
                    } else {
                        NumericChange::Add { amount }
                    },
                });
            }
        }
        // Only numeric modification reaches EX followers; other actions keep the field boundary.
        let subjects = match &modification.on {
            ast::NumericSubjects::Single(objects) => match objects {
                ast::Objects::Zone(query) if matches!(query.zone, ast::Zone::Ex) => {
                    self.zone_objects(query, ZoneKind::Ex, path)?
                }
                ast::Objects::Follow { follow } => self.followed(follow, path)?,
                ast::Objects::This(_)
                | ast::Objects::Bound(_)
                | ast::Objects::Zone(_)
                | ast::Objects::EventHolder { .. } => self.objects(objects, path)?,
            },
            ast::NumericSubjects::Union { any_of } if !set => {
                self.token_follower_union(any_of, path)?
            }
            ast::NumericSubjects::Union { .. } => return Err(unsupported(path)),
        };
        Ok(Action::Modify {
            subjects,
            patch: combined(patches, path)?,
            expires: duration(modification.until.as_ref(), path)?,
            source: EffectReference::CurrentInstallation {},
        })
    }

    fn token_follower_union(
        &mut self,
        queries: &[ast::ObjectQuery; 2],
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let queries = match (&queries[0].zone, &queries[1].zone) {
            (ast::Zone::Field, ast::Zone::Ex) => [&queries[0], &queries[1]],
            (ast::Zone::Ex, ast::Zone::Field) => [&queries[1], &queries[0]],
            _ => return Err(unsupported(path)),
        };
        let mut sets = Vec::with_capacity(2);
        let mut shared_trait: Option<&str> = None;
        for (index, query) in queries.into_iter().enumerate() {
            let ast::ObjectQuery {
                alias: None,
                r#where: None,
                who: ast::Side::Controller,
                zone: ast::Zone::Field | ast::Zone::Ex,
                kind: Some(ast::FieldKind::Follower),
                r#trait: Some(trait_id),
                class: None,
                not_class: None,
                token: Some(true),
                not_token: None,
                orig_cost_eq: None,
                orig_cost_le: None,
                hp_le: None,
                not_trait: None,
                name: None,
                name_contains: None,
                acted: None,
                except: None,
                other: false,
            } = query
            else {
                return Err(unsupported(path));
            };
            if trait_id.is_empty() || shared_trait.is_some_and(|value| value != trait_id.as_str()) {
                return Err(unsupported(path));
            }
            shared_trait = Some(trait_id.as_str());
            let zone = if index == 0 {
                ZoneKind::Field
            } else {
                ZoneKind::Ex
            };
            sets.push(self.zone_objects(query, zone, &format!("{path}/on/any_of/{index}"))?);
        }
        Ok(Selector::Union { sets })
    }

    pub(super) fn continuous(
        &mut self,
        installation: &ast::Apply,
        source: &AbilitySource,
        path: &str,
    ) -> Result<Action, LoweringError> {
        match &installation.effect {
            ast::InstalledEffect::PlayPermission(_) => return Err(unsupported(path)),
            ast::InstalledEffect::LoseAbilities(_) => {
                let ast::Objects::Bound(name) = &installation.on else {
                    return Err(unsupported(path));
                };
                if !matches!(installation.until, None | Some(ast::Until::Turn))
                    || !self.follower_targets.contains(name)
                {
                    return Err(unsupported(path));
                }
                return Ok(Action::RemoveAbility {
                    subjects: self.objects(&installation.on, path)?,
                    scope: RemoveScope::AllAtTime {},
                    duration: duration(installation.until.as_ref(), path)?,
                    timestamp: TimestampBasis::ProperPlay {
                        execution: ExecutionRef::Current {},
                    },
                });
            }
            ast::InstalledEffect::AddTrait(value) => {
                if value.r#trait.is_empty()
                    || !matches!(installation.until, None | Some(ast::Until::Game))
                {
                    return Err(unsupported(path));
                }
                return Ok(Action::Modify {
                    subjects: self.objects(&installation.on, path)?,
                    patch: CharacteristicPatch::AddTrait {
                        trait_id: TraitId(value.r#trait.clone()),
                    },
                    expires: duration(installation.until.as_ref(), path)?,
                    source: EffectReference::CurrentInstallation {},
                });
            }
            ast::InstalledEffect::Cannot(ast::Cannot::PlayAbility(_)) => {
                return self.no_activation(installation, path);
            }
            ast::InstalledEffect::Cannot(ast::Cannot::Attack(_)) => {
                return self.no_attack(installation, path);
            }
            ast::InstalledEffect::Cannot(ast::Cannot::Stand(_)) => {
                return self.no_stand(installation, path);
            }
            ast::InstalledEffect::Cannot(
                ast::Cannot::Player(_) | ast::Cannot::Removal(_) | ast::Cannot::DealDamage(_),
            )
            | ast::InstalledEffect::Prevent(_)
            | ast::InstalledEffect::DamageMod(_) => {
                return self.prevent_apply(installation, source, path);
            }
            ast::InstalledEffect::EvolveCostMod(_) => {
                return self.evolution_discount(installation, path);
            }
            ast::InstalledEffect::Stat(_) | ast::InstalledEffect::CostMod(_) => {}
        }
        let ast::InstalledEffect::Stat(statistics) = &installation.effect else {
            return self.discount(installation, path);
        };
        let mut patches = Vec::new();
        for (stat, value) in [
            (Stat::Attack, statistics.power),
            (Stat::Life, statistics.hp),
        ] {
            if let Some(value) = value {
                patches.push(CharacteristicPatch::Numeric {
                    stat,
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(value)),
                    },
                });
            }
        }
        let objects = self.objects(&installation.on, path)?;
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let duration = duration(installation.until.as_ref(), path)?;
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::Continuous {
                id: EffectDefinitionId(self.card_allocator.id(path)?),
                source: source.clone(),
                target: EffectTarget::Dynamic {
                    selector: Selector::Bound {
                        reference: affected,
                    },
                },
                condition: None,
                patch: combined(patches, path)?,
                layer: ContinuousLayer::Numeric,
                timestamp: TimestampBasis::ProperPlay {
                    execution: ExecutionRef::Current {},
                },
                active_zones: vec![],
                duration: duration.clone(),
            }),
            duration,
        })
    }
}

pub(super) fn pilot(value: &ast::Pilot, path: &str) -> Result<Action, LoweringError> {
    let ast::This::This = value.reference;
    Ok(Action::BoxPilot {
        subject: ObjectOperand::SourceHolder {},
        mode: super::super::BoxPilotMode::Pilot {
            duration: duration(Some(&ast::Until::Turn), path)?,
        },
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Box uses the formal target's checked binding."
)]
impl BodyContext<'_> {
    pub(super) fn boxed(&self, value: &ast::Boxed, path: &str) -> Result<Action, LoweringError> {
        if !matches!(value.until, ast::Until::NextOppTurn) {
            return Err(unsupported(path));
        }
        let (index, selection) = self.selected_parameter(&value.reference, path)?;
        let super::super::ChoiceDomain::Objects { constraint, .. } = &self
            .parameters
            .get(index)
            .ok_or_else(|| unsupported(path))?
            .domain
        else {
            return Err(unsupported(path));
        };
        let [group] = constraint.groups.as_slice() else {
            return Err(unsupported(path));
        };
        let Selector::Zone {
            zone: ZoneKind::Field,
            players,
            binding,
            predicate,
            ..
        } = &group.selector
        else {
            return Err(unsupported(path));
        };
        let follower = super::selection::field_filter(
            &super::selection::ObjectFilter {
                kind: Some(super::super::CardKind::Follower),
                ..Default::default()
            },
            *binding,
        );
        if players.relation != PlayerRelation::Opponents || predicate.as_ref() != &follower {
            return Err(unsupported(path));
        }
        Ok(Action::BoxPilot {
            subject: ObjectOperand::Selected { selection },
            mode: super::super::BoxPilotMode::Boxed {
                duration: duration(Some(&value.until), path)?,
                cutoff: TimestampBasis::ProperPlay {
                    execution: ExecutionRef::Current {},
                },
            },
        })
    }
}
