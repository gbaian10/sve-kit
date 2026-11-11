use super::{Cardinality, LoweringError, Nat, Selector, unsupported};
use crate::ir::{Disjointness, ObjectProperty, SelectionConstraint, SelectionGroup};

#[derive(Clone, Copy, Debug)]
pub(super) enum TotalBounds {
    Explicit(Cardinality),
    Sum,
}

#[derive(Debug)]
pub(super) struct Groups {
    entries: Vec<SelectionGroup>,
    cardinality: Cardinality,
    sum: bool,
}

impl Groups {
    pub(super) const fn new(bounds: TotalBounds) -> Self {
        Self {
            entries: Vec::new(),
            cardinality: match bounds {
                TotalBounds::Explicit(cardinality) => cardinality,
                TotalBounds::Sum => Cardinality {
                    minimum: Nat(0),
                    maximum: Some(Nat(0)),
                },
            },
            sum: matches!(bounds, TotalBounds::Sum),
        }
    }

    pub(super) fn push(
        &mut self,
        cardinality: Cardinality,
        selector: impl FnOnce() -> Result<Selector, LoweringError>,
        path: &str,
    ) -> Result<(), LoweringError> {
        // Search bounds errors precede selector validation and binding allocation.
        if self.sum {
            self.cardinality.minimum = Nat(self
                .cardinality
                .minimum
                .0
                .checked_add(cardinality.minimum.0)
                .ok_or_else(|| unsupported(path))?);
            self.cardinality.maximum = Some(Nat(self
                .cardinality
                .maximum
                .ok_or_else(|| unsupported(path))?
                .0
                .checked_add(cardinality.maximum.ok_or_else(|| unsupported(path))?.0)
                .ok_or_else(|| unsupported(path))?));
        }
        self.entries.push(SelectionGroup {
            selector: selector()?,
            cardinality,
        });
        Ok(())
    }

    pub(super) fn finish(self, distinct_by: Option<ObjectProperty>) -> SelectionConstraint {
        SelectionConstraint {
            groups: self.entries,
            cardinality: self.cardinality,
            distinct_by,
            disjoint: Disjointness::Disjoint,
            aggregate_bounds: vec![],
        }
    }
}
