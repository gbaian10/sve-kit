//! 「自分の墓場が10枚以上で…が場から墓場に置かれたとき」: a departure is observed only when a
//! public zone already met the threshold before the batch moved (BP19-074, Q2590, Q2620).
use super::subject::{SubjectFields, SubjectFilter};
use crate::ir::{
    Binding, Bool, Cardinality, Comparison, Expr, LeaveFieldEvent, Nat, NatRead, ObjectRef,
    PlayerRef, PlayerRelation, PlayerSet, Predicate, SelectionView, Selector, TextQuantifier,
    ZoneKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ZoneCountGate {
    /// The observer's own zone, otherwise its opponent's.
    pub(crate) own: bool,
    pub(crate) zone: ZoneKind,
    pub(crate) comparison: Comparison,
    pub(crate) threshold: Nat,
    pub(crate) member: Binding<ObjectRef>,
}

impl ZoneCountGate {
    fn read(self) -> Expr<Nat> {
        Expr::Live(Box::new(NatRead::ZoneCount {
            selector: Selector::Zone {
                players: PlayerSet {
                    relation: if self.own {
                        PlayerRelation::SelfPlayer
                    } else {
                        PlayerRelation::Opponents
                    },
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
                zone: self.zone,
                view: SelectionView::Current {},
                binding: self.member,
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        }))
    }

    fn predicate(self) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::CompareNat {
            left: self.read(),
            comparison: self.comparison,
            right: Expr::Literal(self.threshold),
        }))
    }

    fn parse(expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(predicate) = expression else {
            return None;
        };
        let Predicate::CompareNat {
            left: Expr::Live(read),
            comparison,
            right: Expr::Literal(threshold),
        } = predicate.as_ref()
        else {
            return None;
        };
        let NatRead::ZoneCount {
            selector:
                Selector::Zone {
                    players,
                    zone,
                    binding,
                    ..
                },
        } = read.as_ref()
        else {
            return None;
        };
        let gate = Self {
            own: players.relation == PlayerRelation::SelfPlayer,
            zone: *zone,
            comparison: *comparison,
            threshold: *threshold,
            member: *binding,
        };
        (matches!(
            gate.zone,
            ZoneKind::Cemetery | ZoneKind::Field | ZoneKind::Ex
        ) && gate.predicate() == *expression)
            .then_some(gate)
    }

    /// Whether `count` cards in the gated zone satisfy the printed threshold.
    pub(crate) const fn admits(self, count: u64) -> bool {
        match self.comparison {
            Comparison::Equal => count == self.threshold.0,
            Comparison::NotEqual => count != self.threshold.0,
            Comparison::Less => count < self.threshold.0,
            Comparison::LessEqual => count <= self.threshold.0,
            Comparison::Greater => count > self.threshold.0,
            Comparison::GreaterEqual => count >= self.threshold.0,
        }
    }
}

/// A field departure filter joined with a pre-departure public zone threshold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DepartureGate {
    pub(crate) subject: SubjectFilter,
    pub(crate) gate: ZoneCountGate,
}

impl DepartureGate {
    pub(crate) fn predicate(&self, fields: SubjectFields) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![self.subject.predicate(fields), self.gate.predicate()],
        }))
    }

    /// Returns the gate and the destination-specific fields its subject filter uses.
    pub(crate) fn parse(
        event: Binding<LeaveFieldEvent>,
        expression: &Expr<Bool>,
    ) -> Option<(Self, SubjectFields)> {
        let Expr::Compute(root) = expression else {
            return None;
        };
        let Predicate::And { predicates } = root.as_ref() else {
            return None;
        };
        let [subject, gate] = predicates.as_slice() else {
            return None;
        };
        let gate = ZoneCountGate::parse(gate)?;
        [
            SubjectFields::Cemetery(event),
            SubjectFields::Hand(event),
            SubjectFields::Leave(event),
        ]
        .into_iter()
        .find_map(|fields| {
            let plan = Self {
                subject: SubjectFilter::parse(fields, subject)?,
                gate,
            };
            (plan.predicate(fields) == *expression).then_some((plan, fields))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CardKind, ScopeId, TraitId};

    #[test]
    fn printed_cemetery_gates_round_trip_through_the_parser() {
        let event = Binding::new(1, ScopeId(1));
        let gate = DepartureGate {
            subject: SubjectFilter {
                own: Some(true),
                itself: Some(false),
                kind: Some(CardKind::Follower),
                trait_id: Some(TraitId("八獄".into())),
                original_cost: Some((Comparison::LessEqual, 3)),
                own_turn: true,
                ..SubjectFilter::default()
            },
            gate: ZoneCountGate {
                own: true,
                zone: ZoneKind::Cemetery,
                comparison: Comparison::GreaterEqual,
                threshold: Nat(10),
                member: Binding::new(2, ScopeId(1)),
            },
        };
        let predicate = gate.predicate(SubjectFields::Cemetery(event));
        let (parsed, fields) = DepartureGate::parse(event, &predicate).unwrap();
        assert_eq!(parsed, gate, "the printed gate survives a parse");
        assert!(
            matches!(fields, SubjectFields::Cemetery(_)),
            "cemetery arrival"
        );
        assert!(gate.gate.admits(10) && !gate.gate.admits(9), "ten or more");
    }
}
