//! Direct stat comparisons preserve rules absence instead of comparing its arithmetic zero.
use super::{BodyContext, Expr, Int, ast};
use crate::ir::{
    CharacteristicView, IntOperation, NumberRead, NumericProperty, ObjectOperand, ObjectView,
    Predicate, Selector, Stat, stat_presence,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Presence uses the same checked source and target bindings as number lowering."
)]
impl BodyContext<'_> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only direct property references require presence; computed scalars already have a value."
    )]
    fn selected_stat_reference(&self, author: &ast::Number, depth: usize) -> bool {
        if depth > self.definitions.len() {
            return false;
        }
        match author {
            ast::Number::Power {
                power_of: ast::Observation::Binding(name),
            } => self.single_targets.contains(name),
            ast::Number::Alias(name) => self
                .definitions
                .get(name)
                .is_some_and(|value| self.selected_stat_reference(value, depth.saturating_add(1))),
            _ => false,
        }
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only direct stat properties carry information presence."
    )]
    pub(super) fn stat_guard(
        &self,
        author: &ast::Number,
        value: &Expr<Int>,
    ) -> Option<Expr<super::Bool>> {
        let Expr::Compute(operation) = value else {
            return None;
        };
        let information = match operation.as_ref() {
            IntOperation::InformationOrZero { information }
                if stat_presence::supported(information) =>
            {
                information.clone()
            }
            IntOperation::Sum {
                objects: Selector::Bound { reference },
                property,
            } if self.selected_stat_reference(author, 0) => {
                let property = match property {
                    NumericProperty::Attack => Stat::Attack,
                    NumericProperty::Life => Stat::Life,
                    _ => return None,
                };
                NumberRead::Stats {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::Selected {
                            selection: *reference,
                        }),
                    },
                    property,
                    view: CharacteristicView::Effective,
                }
            }
            _ => return None,
        };
        Some(Expr::Compute(Box::new(Predicate::HasInfo { information })))
    }
}
