use super::{
    BTreeSet, Binding, BodyContext, Cardinality, Expr, Int, LoweringError, Nat, ObjectView,
    PlayerRef, Selector, ast, unsupported,
};
use crate::ir::ObjectProperty;
use crate::ir::events::timing;
use crate::ir::{
    Capture, CaptureRef, CaptureTime, CardName, CharacteristicView, ChoiceDomain, EventContext,
    IntOperation, NameOperation, NumberRead, PlayerRead, Role, ScopeId, Stat,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Selection captures share the target parameter environment."
)]
impl BodyContext<'_> {
    pub(super) fn selected_parameter(
        &self,
        name: &str,
        path: &str,
    ) -> Result<(usize, Binding<Selector>), LoweringError> {
        let selection = *self.targets.get(name).ok_or_else(|| unsupported(path))?;
        let index = self.parameters.iter().position(|decision| {
            matches!(&decision.domain, ChoiceDomain::Objects { binding, constraint, .. }
                if *binding == selection && constraint.cardinality == Cardinality { minimum: Nat(1), maximum: Some(Nat(1)) })
        }).ok_or_else(|| unsupported(path))?;
        Ok((index, selection))
    }

    pub(super) fn selected_objects(
        &mut self,
        name: &str,
        path: &str,
    ) -> Result<(CaptureTime, CaptureRef<Selector>), LoweringError> {
        let (index, selection) = self.selected_parameter(name, path)?;
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, selection.key().0),
        };
        let decision = self
            .parameters
            .get_mut(index)
            .ok_or_else(|| unsupported(path))?;
        let at = CaptureTime::AtSelection {
            decision: decision.id,
        };
        decision.captures.push(Capture::Objects {
            at,
            value: Selector::Bound {
                reference: selection,
            },
            into: into.clone(),
        });
        Ok((at, into))
    }

    /// Captures the chosen object's player at selection, so later moves cannot change it.
    pub(super) fn selected_player(
        &mut self,
        name: &str,
        role: Role,
        path: &str,
    ) -> Result<PlayerRef, LoweringError> {
        let (index, selection) = self.selected_parameter(name, path)?;
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, selection.key().0),
        };
        let decision = self
            .parameters
            .get_mut(index)
            .ok_or_else(|| unsupported(path))?;
        decision.captures.push(Capture::Player {
            at: CaptureTime::AtSelection {
                decision: decision.id,
            },
            value: Expr::Live(Box::new(PlayerRead::Role {
                object: ObjectView::Selected { selection },
                role,
            })),
            into: into.clone(),
        });
        Ok(PlayerRef::FromExpression {
            value: Box::new(Expr::Captured(into)),
        })
    }

    /// Who answers or pays: the effect controller, its opponent, or a chosen object's
    /// controller captured at selection, so a later move cannot change the decider.
    pub(super) fn participant(
        &mut self,
        who: Option<&ast::Participant>,
        path: &str,
    ) -> Result<PlayerRef, LoweringError> {
        match who {
            None | Some(ast::Participant::Side(ast::LeaderSide::Controller)) => {
                Ok(PlayerRef::Controller {})
            }
            Some(ast::Participant::Side(ast::LeaderSide::Opponent)) => Ok(timing::opponent()),
            Some(ast::Participant::MasterOf { master_of }) => {
                self.selected_player(master_of, Role::Controller, path)
            }
        }
    }

    pub(super) fn selected_name(
        &mut self,
        name: &str,
        path: &str,
    ) -> Result<Expr<CardName>, LoweringError> {
        let (index, selection) = self.selected_parameter(name, path)?;
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, selection.key().0),
        };
        let decision = self
            .parameters
            .get_mut(index)
            .ok_or_else(|| unsupported(path))?;
        decision.captures.push(Capture::Name {
            at: CaptureTime::AtSelection {
                decision: decision.id,
            },
            value: Expr::Compute(Box::new(NameOperation::UniqueName {
                object: ObjectView::Selected { selection },
            })),
            into: into.clone(),
        });
        Ok(Expr::Captured(into))
    }

    pub(super) fn selected_stat(
        &mut self,
        name: &str,
        value: &ast::SelectedStat,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        if let ast::SelectedStat::DistinctOriginalCosts(collection) = value {
            return self.selected_original_cost_count(name, &collection.r#in, path);
        }
        let reference = match value {
            ast::SelectedStat::Hp(reference)
            | ast::SelectedStat::Power(reference)
            | ast::SelectedStat::Cost(reference)
            | ast::SelectedStat::OriginalCost(reference) => reference,
            ast::SelectedStat::DistinctOriginalCosts(_) => return Err(unsupported(path)),
        };
        if reference != name {
            return Err(unsupported(path));
        }
        let (index, selection) = self.selected_parameter(name, path)?;
        let information = match value {
            ast::SelectedStat::Hp(_) | ast::SelectedStat::Power(_) => NumberRead::Stats {
                object: ObjectView::Selected { selection },
                property: if matches!(value, ast::SelectedStat::Hp(_)) {
                    Stat::Life
                } else {
                    Stat::Attack
                },
                view: CharacteristicView::Effective,
            },
            ast::SelectedStat::Cost(_) | ast::SelectedStat::OriginalCost(_) => NumberRead::Cost {
                object: ObjectView::Selected { selection },
                view: if matches!(value, ast::SelectedStat::OriginalCost(_)) {
                    CharacteristicView::Original
                } else {
                    CharacteristicView::Effective
                },
            },
            ast::SelectedStat::DistinctOriginalCosts(_) => return Err(unsupported(path)),
        };
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, selection.key().0),
        };
        let decision = self
            .parameters
            .get_mut(index)
            .ok_or_else(|| unsupported(path))?;
        decision.captures.push(Capture::Integer {
            at: CaptureTime::AtSelection {
                decision: decision.id,
            },
            value: Expr::Compute(Box::new(IntOperation::InformationOrZero { information })),
            into,
        });
        Ok(Expr::Captured(into))
    }

    fn selected_original_cost_count(
        &mut self,
        name: &str,
        reference: &str,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        if reference != name
            || self.observed_targets.contains(name)
            || self.hand_targets.contains(name)
        {
            return Err(unsupported(path));
        }
        let selection = *self.targets.get(name).ok_or_else(|| unsupported(path))?;
        let decision = self.parameters.iter_mut().find(|decision| {
            matches!(&decision.domain, ChoiceDomain::Objects { binding, .. } if *binding == selection)
        }).ok_or_else(|| unsupported(path))?;
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, selection.key().0),
        };
        decision.captures.push(Capture::Integer {
            at: CaptureTime::AtSelection {
                decision: decision.id,
            },
            value: super::numeric::aggregate_count(
                Selector::Bound {
                    reference: selection,
                },
                Some(ObjectProperty::Cost),
            ),
            into,
        });
        Ok(Expr::Captured(into))
    }

    pub(super) fn resolution_count(
        &mut self,
        value: &ast::Number,
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        let cemetery = match value {
            ast::Number::Count { count } => matches!(count.zone, ast::CountZone::Cemetery),
            ast::Number::DistinctNames {
                distinct_names: ast::DistinctQuery::Public(query),
            }
            | ast::Number::DistinctOriginalCosts {
                distinct_orig_costs: ast::DistinctQuery::Public(query),
            } => matches!(query.zone, ast::PublicZone::Cemetery),
            ast::Number::DistinctNames {
                distinct_names: ast::DistinctQuery::Receipt { .. },
            }
            | ast::Number::DistinctOriginalCosts {
                distinct_orig_costs: ast::DistinctQuery::Receipt { .. },
            }
            | ast::Number::Dice { .. }
            | ast::Number::Counter { .. }
            | ast::Number::Power { .. }
            | ast::Number::Life { .. }
            | ast::Number::Cost { .. }
            | ast::Number::OriginalCost { .. }
            | ast::Number::Literal(_)
            | ast::Number::Alias(_)
            | ast::Number::Aggregate { .. }
            | ast::Number::HistoryCount { .. }
            | ast::Number::HistorySum { .. }
            | ast::Number::ReceiptCount { .. }
            | ast::Number::CountOf { .. }
            | ast::Number::PaidAmount { .. }
            | ast::Number::TurnOrdinal { .. }
            | ast::Number::LeaderLife { .. }
            | ast::Number::Plus { .. }
            | ast::Number::Minus { .. }
            | ast::Number::Times { .. }
            | ast::Number::HalfUp { .. }
            | ast::Number::Per { .. }
            | ast::Number::Min { .. }
            | ast::Number::Event { .. }
            | ast::Number::Resource { .. }
            | ast::Number::Resolution { .. }
            | ast::Number::Selection { .. } => false,
        };
        if !cemetery {
            return Err(unsupported(path));
        }
        let previous = self.scope;
        self.scope = self.resolution_scope;
        let lowered = self.number(value, path, active);
        self.scope = previous;
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, self.resolution_scope),
        };
        self.resolution_captures.push(Capture::Integer {
            at: CaptureTime::AtResolution {
                scope: self.resolution_scope,
            },
            value: lowered?,
            into,
        });
        Ok(Expr::Captured(into))
    }
    pub(super) fn event_count(
        &mut self,
        value: &ast::Number,
        path: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Expr<Int>, LoweringError> {
        use crate::ir::history_turn::{evolution_capture_query, evolution_trigger};
        let trigger = self
            .event_trigger
            .as_ref()
            .filter(|trigger| self.auto.is_some() && evolution_trigger(trigger))
            .ok_or_else(|| unsupported(path))?;
        let at = CaptureTime::AtEvent {
            event: EventContext::Trigger {},
            policy: trigger.view,
        };
        if !matches!(value, ast::Number::HistoryCount { .. }) {
            return Err(unsupported(path));
        }
        let previous = self.scope;
        self.scope = ScopeId(self.definition.0);
        let lowered = self.number(value, path, active);
        self.scope = previous;
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, ScopeId(self.definition.0)),
        };
        let capture = Capture::Integer {
            at,
            value: lowered?,
            into,
        };
        if evolution_capture_query(&capture, at).is_none() {
            return Err(unsupported(path));
        }
        self.event_captures.push(capture);
        Ok(Expr::Captured(into))
    }
}
