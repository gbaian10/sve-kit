use super::{Execution, Game, Result, WellFormedCardProgram, invalid, unsupported};
use crate::ir::quick::card_permission;
use crate::ir::{AbilityKind, FaceId, Located, PlayCostPayment};
use crate::ir::{
    AbilityOrigin, AbilitySource, Arithmetic, CostSpec, ExecutionRef, ObjectOperand, ZoneKind,
};
use crate::ir::{
    Bool, BoolRead, CostChange, Expr, Int, IntOperation, IntRead, Nat, NatOperation, NatRead,
    ObjectProperty, PlayerRef, PlayerResource, Predicate, Selector, SourceSpan,
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
            if !matches!(
                printed["card_type"].as_str(),
                Some("フォロワー" | "スペル" | "スペル・トークン")
            ) {
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
        if let Some(payment) = &face.node.additional {
            check_additional(game, card_no, card, &face.node.face, payment)
                .map_err(|error| at_source(error, &payment.source))?;
        }
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
                match &modifier.node.change {
                    CostChange::Delta { amount } | CostChange::Set { amount } => {
                        check_integer(amount)?;
                    }
                }
                if let Some(condition) = &modifier.node.condition {
                    check_condition(condition, true)?;
                }
                Ok(())
            };
            check_modifier().map_err(|error| at_source(error, &modifier.source))?;
        }
    }
    Ok(())
}

fn check_additional(
    game: &Game,
    card_no: &str,
    card: &WellFormedCardProgram,
    face: &FaceId,
    payment: &Located<PlayCostPayment>,
) -> Result<()> {
    if !matches!(
        game.catalog.face(card_no, 0)?["card_type"].as_str(),
        Some("スペル" | "スペル・トークン")
    ) {
        return Err(unsupported("additional cost requires a Spell"));
    }
    for ability in &card.card().abilities {
        if ability.node.kind == AbilityKind::Spell {
            super::play::check_additional_body(&ability.node)?;
        }
    }
    let quick = card
        .card()
        .abilities
        .iter()
        .any(|ability| card_permission(&ability.node));
    let hand = quick
        && super::hand_payment::single_additional(&payment.node.play.costs)
        && payment.node.play.parameters.is_empty();
    if quick && !hand {
        return Err(unsupported("additional Quick payment preflight"));
    }
    let CostSpec::Ordered { steps } = &payment.node.play.costs else {
        return Err(unsupported("additional ordered costs"));
    };
    if !hand
        && (steps.is_empty()
            || steps.iter().any(|step| {
                !super::material::check_cost(&step.instruction.action, &payment.node.play)
                    || !matches!(
                        super::material::cost_objects(&step.instruction.action),
                        Some(Selector::Bound { .. })
                    )
            }))
    {
        return Err(unsupported("additional public material costs"));
    }
    game.check_card_source(
        &payment.source,
        card_no,
        &face.0,
        u64::from(payment.source.line),
    )?;
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Printed {
            face: face.clone(),
            line: Nat(u64::from(payment.source.line)),
        },
        controller: PlayerRef::Controller {},
    };
    super::check_payment(&payment.node.play, &origin, false, &payment.source)?;
    for decision in &payment.node.play.parameters {
        super::material::check_parameter(decision)?;
    }
    if payment
        .node
        .play
        .parameters
        .iter()
        .any(|parameter| !super::material::is_parameter(parameter))
    {
        return Err(unsupported("additional public materials"));
    }
    Ok(())
}

pub(super) fn check_condition(condition: &Expr<Bool>, play_origin: bool) -> Result<()> {
    match condition {
        Expr::Literal(_) => Ok(()),
        Expr::Live(read)
            if matches!(
                read.as_ref(),
                BoolRead::Crimson {
                    player: PlayerRef::Controller {}
                }
            ) || (play_origin
                && matches!(
                    read.as_ref(),
                    BoolRead::CardPlayOrigin {
                        execution: ExecutionRef::Current {},
                        zone: ZoneKind::Hand | ZoneKind::Ex
                    }
                )) =>
        {
            Ok(())
        }
        Expr::Compute(predicate) => match predicate.as_ref() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                for item in predicates {
                    check_condition(item, play_origin)?;
                }
                Ok(())
            }
            Predicate::Not { predicate } => check_condition(predicate, play_origin),
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
            Predicate::PlayerEquals {
                left: Expr::Literal(PlayerRef::Controller {}),
                right: Expr::Literal(PlayerRef::Active {}),
            } => Ok(()),
            Predicate::Exists { objects } => check_objects(objects),
            _ => Err(unsupported("play cost public condition")),
        },
        _ => Err(unsupported("play cost public condition")),
    }
}

fn check_integer(value: &Expr<Int>) -> Result<()> {
    match value {
        Expr::Live(read)
            if matches!(
                read.as_ref(),
                IntRead::PlayerResource {
                    property: PlayerResource::Life | PlayerResource::PpMaximum,
                    ..
                }
            ) =>
        {
            Ok(())
        }
        Expr::Literal(_) => Ok(()),
        Expr::Compute(operation) => match operation.as_ref() {
            IntOperation::FromNat { value: count } => check_count(count),
            IntOperation::Arithmetic {
                operation:
                    Arithmetic::Subtract
                    | Arithmetic::Multiply
                    | Arithmetic::FloorDivide
                    | Arithmetic::Minimum,
                arguments,
            } => {
                for argument in arguments {
                    check_integer(argument)?;
                }
                Ok(())
            }
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
            NatRead::TurnOrdinal { .. } => Ok(()),
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
            .definitions
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
                    CostChange::Delta { amount } => {
                        delta = delta
                            .checked_add(execution.integer(self, amount)?.0)
                            .ok_or_else(|| invalid("play cost overflow"))?;
                    }
                    CostChange::Set {
                        amount: Expr::Literal(Int(amount)),
                    } => base = *amount,
                    CostChange::Set { .. } => return Err(unsupported("play cost change")),
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
        self.internal_other_play_cost(execution, base, delta)
            .map(|cost| cost.max(0))
    }
}

fn condition_value(game: &Game, execution: &Execution, condition: &Expr<Bool>) -> Result<bool> {
    execution.boolean(game, condition).map(|Bool(value)| value)
}
