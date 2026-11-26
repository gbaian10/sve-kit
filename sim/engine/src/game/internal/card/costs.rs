use super::super::number_plan::{CostAmountPlan, CostConditionPlan};
use super::{Execution, Game, Result, WellFormedCardProgram, invalid, unsupported};
use crate::ir::flip_cost as flip_payment;
use crate::ir::play_target_cost;
use crate::ir::quick::card_permission;
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::{
    AbilityKind, Bool, CostOverride, FaceId, Located, PlayCostExchangeChange, PlayCostPayment,
};
use crate::ir::{AbilityOrigin, AbilitySource, CostSpec, ObjectOperand};
use crate::ir::{CostChange, Expr, Int, Nat, PlayerRef, Selector, SourceSpan};
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
            if let PlayCostExchangeChange::ReplaceOriginal { condition, .. } = &exchange.node.change
            {
                CostConditionPlan::compile(condition, true)
                    .map_err(|error| at_source(error, &exchange.source))?;
            }
            for (_, step) in exchange
                .node
                .routes()
                .ok_or_else(|| unsupported("exchange routes"))?
            {
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
                        CostAmountPlan::compile(amount)?;
                    }
                }
                if let Some(condition) = &modifier.node.condition {
                    check_target_condition(condition, card, &modifier.source)?;
                }
                Ok(())
            };
            check_modifier().map_err(|error| at_source(error, &modifier.source))?;
        }
    }
    Ok(())
}

fn check_target_condition(
    condition: &Expr<Bool>,
    card: &WellFormedCardProgram,
    source: &SourceSpan,
) -> Result<()> {
    let mut inspected = condition.clone();
    if !play_target_cost::mask(&mut inspected, card.card(), source) {
        return Err(unsupported("foreign play target cost dependency"));
    }
    CostConditionPlan::compile(condition, true)?;
    Ok(())
}

fn check_additional(
    game: &Game,
    card_no: &str,
    card: &WellFormedCardProgram,
    face: &FaceId,
    payment: &Located<PlayCostPayment>,
) -> Result<()> {
    let printed = game.catalog.face(card_no, 0)?;
    let kind = printed["card_type"].as_str();
    if !matches!(
        kind,
        Some("スペル" | "スペル・トークン" | "フォロワー" | "アミュレット")
    ) {
        return Err(unsupported("additional cost card kind"));
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
    let hand = (quick || matches!(kind, Some("スペル" | "スペル・トークン")))
        && super::hand_payment::single_additional(&payment.node.play.costs)
        && payment.node.play.parameters.is_empty();
    let flips = !payment.node.play.parameters.is_empty()
        && payment
            .node
            .play
            .parameters
            .iter()
            .all(|decision| flip_payment::material(decision).is_some());
    if quick && !hand && !flips {
        return Err(unsupported("additional Quick payment preflight"));
    }
    if reveal_payment::payment(&payment.node.play) {
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
        let CostSpec::Ordered { steps } = &payment.node.play.costs else {
            return Err(unsupported("mandatory revelation ordered costs"));
        };
        if steps.iter().any(|step| step.instruction.source != origin) {
            return Err(unsupported("mandatory revelation source"));
        }
        let [decision] = payment.node.play.parameters.as_slice() else {
            return Err(unsupported("one mandatory revelation parameter"));
        };
        return super::material::check_reveal_parameter(decision);
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
        exchange: Option<&PlayCostExchangeChange>,
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

    fn quote_card_cost(
        &self,
        execution: &Execution,
        exchange: Option<&PlayCostExchangeChange>,
    ) -> Result<i64> {
        let holder = &execution.context.source;
        let object = self.object(holder)?;
        let has_prototype_layers = !self.state.continuous.is_empty();
        if object.state.get("cost").is_some() || has_prototype_layers {
            // A saved cost override belongs to this card and cannot witness typed payment.
            if !has_prototype_layers && execution.probing_declaration() {
                return Err(crate::EngineFailure::IllegalPlay(
                    "cannot prove payment with a prototype cost override".into(),
                ));
            }
            return Err(unsupported("typed payment state or prototype cost layers"));
        }
        let printed_base = super::super::cost::printed_play_cost(&self.face(holder)?["cost"])?;
        let mut base =
            if let Some(CostOverride::SetBase { amount }) = execution.cost_override.as_ref() {
                amount.0
            } else {
                printed_base
            };
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
                        CostConditionPlan::compile(condition, true)?.evaluate(execution, self)
                    })?;
                if !applies {
                    continue;
                }
                match &modifier.node.change {
                    CostChange::Delta { amount } => {
                        delta = delta
                            .checked_add(
                                CostAmountPlan::compile(amount)?.evaluate(execution, self)?,
                            )
                            .ok_or_else(|| invalid("play cost overflow"))?;
                    }
                    CostChange::Set {
                        amount: Expr::Literal(Int(amount)),
                    } => base = *amount,
                    CostChange::Set { .. } => return Err(unsupported("play cost change")),
                }
            }
        }
        if let Some(PlayCostExchangeChange::Adjust(change)) = exchange {
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
        let (base, delta) = self.internal_other_play_cost_parts(execution, base, delta)?;
        let base = if matches!(
            exchange,
            Some(PlayCostExchangeChange::ReplaceOriginal { .. })
        ) {
            0
        } else {
            base
        };
        base.checked_add(delta)
            .map(|cost| cost.max(0))
            .ok_or_else(|| invalid("play cost overflow"))
    }
}
