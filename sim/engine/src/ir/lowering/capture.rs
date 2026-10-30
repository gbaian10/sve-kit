use super::{
    BTreeSet, Binding, BodyContext, Cardinality, Expr, Int, LoweringError, Nat, ObjectView,
    PlayerRef, Selector, ast, unsupported,
};
use crate::ir::{
    Capture, CaptureRef, CaptureTime, CharacteristicView, ChoiceDomain, IntOperation, NumberRead,
    PlayerRead, Role, Stat,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Selection captures share the target parameter environment."
)]
impl BodyContext<'_> {
    fn selected_parameter(
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

    pub(super) fn selected_controller(
        &mut self,
        name: &str,
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
                role: Role::Controller,
            })),
            into: into.clone(),
        });
        Ok(PlayerRef::FromExpression {
            value: Box::new(Expr::Captured(into)),
        })
    }

    pub(super) fn selected_stat(
        &mut self,
        name: &str,
        value: &ast::SelectedStat,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let (reference, property) = match value {
            ast::SelectedStat::HpOf(reference) => (reference, Stat::Life),
            ast::SelectedStat::PowerOf(reference) => (reference, Stat::Attack),
        };
        if reference != name {
            return Err(unsupported(path));
        }
        let (index, selection) = self.selected_parameter(name, path)?;
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
            value: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                information: NumberRead::Stats {
                    object: ObjectView::Selected { selection },
                    property,
                    view: CharacteristicView::Effective,
                },
            })),
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
        if !matches!(value, ast::Number::Count { count } if matches!(count.zone, ast::CountZone::Cemetery))
        {
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
}
