use super::{Execution, Game, Result, WellFormedCardProgram, invalid, unsupported};
use crate::game::internal::values::compare;
use crate::ir::{AbilityOrigin, AbilityQualification, AbilitySource, CostSpec, ObjectOperand};
use crate::ir::{
    Bool, CostChange, Expr, Int, IntOperation, Nat, NatOperation, NatRead, ObjectProperty,
    PlayerRef, Predicate, Selector, SourceSpan,
};
use alloc::collections::BTreeSet;

fn at_source(error: crate::EngineFailure, source: &SourceSpan) -> crate::EngineFailure {
    match error {
        crate::EngineFailure::Invalid(reason) => invalid(format!("{reason}; {source:?}")),
        other => unsupported(&format!("{other}; {source:?}")),
    }
}

pub(super) fn check(game: &Game, card_no: &str, card: &WellFormedCardProgram) -> Result<()> {
    let mut faces = BTreeSet::new();
    for face in &card.card().card_play_costs {
        let mut check = || {
            if game.printed_face_ordinal(card_no, &face.node.face)? != 0
                || !faces.insert(game.printed_face_ordinal(card_no, &face.node.face)?)
            {
                return Err(unsupported("play cost requires base face"));
            }
            let printed = game.catalog.face(card_no, 0)?;
            if !matches!(printed["card_type"].as_str(), Some("フォロワー" | "スペル")) {
                return Err(unsupported("play cost card kind"));
            }
            super::super::cost::printed_play_cost(&printed["cost"])?;
            game.check_card_source(
                &face.source,
                card_no,
                &face.node.face.0,
                u64::from(face.source.line),
            )?;
            Ok(())
        };
        check().map_err(|error| at_source(error, &face.source))?;
        if let Some(exchange) = &face.node.exchange {
            if exchange.source.line == 0
                || exchange.source.scope.is_empty()
                || exchange.source.author_path.is_empty()
            {
                return Err(at_source(
                    invalid("incomplete exchange source"),
                    &exchange.source,
                ));
            }
            if card
                .card()
                .abilities
                .iter()
                .any(|ability| ability.node.qualification != AbilityQualification::Ordinary)
            {
                return Err(unsupported("exchange requires ordinary card play"));
            }
            game.check_card_source(
                &exchange.source,
                card_no,
                &face.node.face.0,
                u64::from(exchange.source.line),
            )
            .map_err(|error| at_source(error, &exchange.source))?;
            let CostSpec::Ordered { steps } = &exchange.node.cost else {
                return Err(unsupported("exchange cost"));
            };
            let [step] = steps.as_slice() else {
                return Err(unsupported("exchange steps"));
            };
            if step.instruction.source
                != (AbilitySource {
                    holder: ObjectOperand::SourceHolder {},
                    origin: AbilityOrigin::Printed {
                        face: face.node.face.clone(),
                        line: Nat(u64::from(exchange.source.line)),
                    },
                    controller: PlayerRef::Controller {},
                })
            {
                return Err(unsupported("exchange instruction source"));
            }
        }
        for modifier in &face.node.modifiers {
            let check_modifier = || {
                if modifier.source.line == 0
                    || modifier.source.scope.is_empty()
                    || modifier.source.author_path.is_empty()
                {
                    return Err(invalid("incomplete play cost source"));
                }
                game.check_card_source(
                    &modifier.source,
                    card_no,
                    &face.node.face.0,
                    u64::from(modifier.source.line),
                )?;
                if let Some(condition) = &modifier.node.condition {
                    check_condition(condition)?;
                }
                Ok(())
            };
            check_modifier().map_err(|error| at_source(error, &modifier.source))?;
        }
    }
    Ok(())
}

fn check_condition(condition: &Expr<Bool>) -> Result<()> {
    match condition {
        Expr::Literal(_) => Ok(()),
        Expr::Compute(predicate) => match predicate.as_ref() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                for item in predicates {
                    check_condition(item)?;
                }
                Ok(())
            }
            Predicate::Not { predicate } => check_condition(predicate),
            Predicate::CompareNat {
                left,
                right: Expr::Literal(_),
                ..
            } => check_count(left),
            Predicate::CompareInt {
                left,
                right: Expr::Literal(_),
                ..
            } => check_integer(left),
            Predicate::Exists { objects } => check_objects(objects),
            _ => Err(unsupported("play cost public condition")),
        },
        _ => Err(unsupported("play cost public condition")),
    }
}

fn check_integer(value: &Expr<Int>) -> Result<()> {
    match value {
        Expr::Literal(_) => Ok(()),
        Expr::Compute(operation) => match operation.as_ref() {
            IntOperation::FromNat { value: count } => check_count(count),
            _ => Err(unsupported("play cost count operand")),
        },
        _ => Err(unsupported("play cost count operand")),
    }
}

fn check_count(value: &Expr<Nat>) -> Result<()> {
    match value {
        Expr::Literal(_) => Ok(()),
        Expr::Live(read) => match read.as_ref() {
            NatRead::ZoneCount { selector } => check_objects(selector),
            _ => Err(unsupported("play cost public count")),
        },
        Expr::Compute(operation) => match operation.as_ref() {
            NatOperation::DistinctCount {
                objects,
                property: ObjectProperty::Names | ObjectProperty::Cost,
            } => check_objects(objects),
            _ => Err(unsupported("play cost public count")),
        },
        _ => Err(unsupported("play cost public count")),
    }
}

fn check_objects(objects: &Selector) -> Result<()> {
    super::selection::public_zone_selector(objects)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Payment replacements use the same authoritative Resolution state."
)]
impl Game {
    pub(in crate::game::internal) fn internal_card_play_cost(
        &self,
        execution: &Execution,
    ) -> Result<i64> {
        self.internal_card_play_cost_with(execution, None)
    }

    pub(super) fn internal_card_play_cost_with(
        &self,
        execution: &Execution,
        exchange: Option<&CostChange>,
    ) -> Result<i64> {
        let holder = &execution.context.source;
        let object = self.object(holder)?;
        if object.zone != "resolution" {
            return Err(unsupported("typed payment zone"));
        }
        self.quote_card_cost(execution, exchange)
    }

    pub(in crate::game::internal) fn internal_quote_card_cost(
        &self,
        execution: &Execution,
    ) -> Result<i64> {
        self.quote_card_cost(execution, None)
    }

    fn quote_card_cost(&self, execution: &Execution, exchange: Option<&CostChange>) -> Result<i64> {
        let holder = &execution.context.source;
        let object = self.object(holder)?;
        if object.state.get("cost").is_some() || !self.state.continuous.is_empty() {
            return Err(unsupported("typed payment state or prototype cost layers"));
        }
        let mut base = super::super::cost::printed_play_cost(&self.face(holder)?["cost"])?;
        let mut delta = 0_i64;
        let card = self
            .card_runtime
            .cards
            .get(&object.card)
            .ok_or_else(|| invalid("card is not loaded"))?;
        for face in &card.card().card_play_costs {
            for modifier in &face.node.modifiers {
                let applies = modifier
                    .node
                    .condition
                    .as_ref()
                    .map_or(Ok(true), |condition| {
                        condition_value(self, execution, condition)
                    })?;
                if !applies {
                    continue;
                }
                match &modifier.node.change {
                    CostChange::Delta {
                        amount: Expr::Literal(Int(amount)),
                    } => {
                        delta = delta
                            .checked_add(*amount)
                            .ok_or_else(|| invalid("play cost overflow"))?;
                    }
                    CostChange::Set {
                        amount: Expr::Literal(Int(amount)),
                    } => base = *amount,
                    _ => return Err(unsupported("play cost change")),
                }
            }
        }
        if let Some(change) = exchange {
            match change {
                CostChange::Set {
                    amount: Expr::Literal(Int(amount)),
                } => base = *amount,
                CostChange::Delta {
                    amount: Expr::Literal(Int(amount)),
                } => {
                    delta = delta
                        .checked_add(*amount)
                        .ok_or_else(|| invalid("play cost overflow"))?;
                }
                _ => return Err(unsupported("exchange change")),
            }
        }
        let raw = base
            .checked_add(delta)
            .ok_or_else(|| invalid("play cost overflow"))?;
        self.internal_installed_play_cost(holder, raw)
            .map(|cost| cost.max(0))
    }
}

fn condition_value(game: &Game, execution: &Execution, condition: &Expr<Bool>) -> Result<bool> {
    match condition {
        Expr::Literal(Bool(value)) => Ok(*value),
        Expr::Compute(predicate) => match predicate.as_ref() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                let values = predicates
                    .iter()
                    .map(|item| condition_value(game, execution, item))
                    .collect::<Result<Vec<_>>>()?;
                Ok(if matches!(predicate.as_ref(), Predicate::And { .. }) {
                    values.iter().all(|value| *value)
                } else {
                    values.iter().any(|value| *value)
                })
            }
            Predicate::Not { predicate } => Ok(!condition_value(game, execution, predicate)?),
            Predicate::CompareNat {
                left,
                comparison,
                right: Expr::Literal(right),
            } => Ok(compare(count(game, execution, left)?, *comparison, right.0)),
            Predicate::CompareInt {
                left,
                comparison,
                right: Expr::Literal(right),
            } => {
                let left = match left {
                    Expr::Literal(Int(value)) => *value,
                    Expr::Compute(operation) => match operation.as_ref() {
                        IntOperation::FromNat { value } => {
                            i64::try_from(count(game, execution, value)?).map_err(invalid)?
                        }
                        _ => return Err(unsupported("play cost count operand")),
                    },
                    _ => return Err(unsupported("play cost count operand")),
                };
                Ok(compare(left, *comparison, right.0))
            }
            Predicate::Exists { objects } => Ok(object_count(game, execution, objects)? > 0),
            _ => Err(unsupported("play cost public condition")),
        },
        _ => Err(unsupported("play cost public condition")),
    }
}

fn count(game: &Game, execution: &Execution, value: &Expr<Nat>) -> Result<u64> {
    execution.natural(game, value).map(|Nat(amount)| amount)
}

fn object_count(game: &Game, execution: &Execution, objects: &Selector) -> Result<u64> {
    u64::try_from(execution.public_zone_objects(game, objects)?.len()).map_err(invalid)
}
