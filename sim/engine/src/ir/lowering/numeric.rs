use super::super::{
    CharacteristicPatch, ContinuousLayer, Duration, DurationEnd, Effect, EffectDefinitionId,
    EffectReference, EffectTarget, ExecutionRef, InstallationSubjects, InstallationTime, Interval,
    NumericChange, ObjectRef, Stat, TimeAnchor, TimestampBasis,
};
use super::{
    AbilitySource, Action, BTreeSet, Binding, BodyContext, Cardinality, Expr, Int, LoweringError,
    Nat, ObjectOperand, PlayerRef, PlayerRelation, PlayerSet, SelectionView, Selector,
    TextQuantifier, ZoneKind, ast, unsupported,
};

pub(super) fn holder() -> Selector {
    Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    }
}

pub(super) fn duration(until: Option<&ast::Until>) -> Duration {
    Duration {
        end: match until {
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
    }
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
    pub(super) fn objects(
        &mut self,
        value: &ast::Objects,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        let query = match value {
            ast::Objects::This(_) => return Ok(holder()),
            ast::Objects::Bound(name) => {
                return self
                    .targets
                    .get(name)
                    .copied()
                    .map(|reference| Selector::Bound { reference })
                    .ok_or_else(|| unsupported(path));
            }
            ast::Objects::Follow { .. } => return Err(unsupported(path)),
            ast::Objects::Zone(query) => query,
        };
        let binding: Binding<ObjectRef> = Binding::new(self.allocator.id(path)?, self.scope);
        let kind = query.kind.as_ref().map(|_| ast::SearchKind::Follower);
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
            zone: match query.zone {
                ast::Zone::Field => ZoneKind::Field,
            },
            view: SelectionView::Current {},
            binding,
            predicate: Box::new(super::search::object_filter(
                kind.as_ref(),
                None,
                None,
                binding,
            )),
        };
        Ok(if query.other {
            Selector::Difference {
                input: Box::new(objects),
                except: Box::new(holder()),
            }
        } else {
            objects
        })
    }

    pub(super) fn action_objects(
        &mut self,
        action: &ast::ObjectAction,
        path: &str,
    ) -> Result<Selector, LoweringError> {
        match action {
            ast::ObjectAction::Short(objects) | ast::ObjectAction::Long { r#ref: objects } => {
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
        let mut patches = Vec::new();
        for (stat, value) in [
            (Stat::Attack, &modification.power),
            (Stat::Life, &modification.hp),
        ] {
            if let Some(value) = value {
                let amount = self.number(value, path, &mut BTreeSet::new())?;
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
        Ok(Action::Modify {
            subjects: self.objects(&modification.on, path)?,
            patch: combined(patches, path)?,
            expires: duration(modification.until.as_ref()),
            source: EffectReference::CurrentInstallation {},
        })
    }

    pub(super) fn continuous(
        &mut self,
        installation: &ast::Apply,
        source: &AbilitySource,
        path: &str,
    ) -> Result<Action, LoweringError> {
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
        let duration = duration(installation.until.as_ref());
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
