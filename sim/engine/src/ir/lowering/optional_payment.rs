use super::{AbilitySource, BodyContext, Located, LoweringError, Node, SourceSpan, ast, payment};
use crate::ir::{
    Action, Control, CostSpec, Decision, PaymentOptionality, PaymentTime, PlayerRef, Program,
    ReceiptId,
};
use core::slice;

pub(super) struct PaymentDeclaration {
    pub node: Located<Node>,
    pub parameters: Vec<Decision>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Optional payment shares the body binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn resolution_payment(
        &mut self,
        payment: &ast::ModePayment,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        if self.auto.is_none() {
            return Err(super::unsupported(path));
        }
        let payer = self.participant(payment.who.as_ref(), path)?;
        let operations = match &payment.cost {
            ast::Cost::Short(operation) => slice::from_ref(operation.as_ref()),
            ast::Cost::Ordered { steps } => steps.as_slice(),
        };
        // A fixed positive PP amount is the payer's own resource, so any player may pay it.
        let player_pp =
            matches!(operations, [ast::CostOperation::Pp(ast::PpAmount::Literal(n))] if *n > 0);
        let source_costs = payer == (PlayerRef::Controller {})
            && !operations.is_empty()
            && operations.iter().all(|operation| {
                matches!(
                    operation,
                    ast::CostOperation::ActSelf(true) | ast::CostOperation::SelfToCemetery(true)
                )
            });
        if !player_pp && !source_costs {
            return Err(super::unsupported(path));
        }
        let mut play = payment::lower(self, &payment.cost, origin, path)?;
        if payer != (PlayerRef::Controller {}) {
            pay_as(&mut play.costs, &payer, path)?;
        }
        let scope = self.scope;
        self.scope = self.card_allocator.scope(path)?;
        let body = self.steps(&payment.steps, path, source, origin);
        self.scope = scope;
        let receipt = ReceiptId(self.card_allocator.id(path)?);
        if let Some(result) = &payment.result {
            self.bind_payment(result, receipt, path)?;
        }
        Ok(Self::located_step(
            source,
            path,
            Node::Control {
                control: Control::Pay {
                    at: PaymentTime::ResolutionChoice,
                    costs: play.costs,
                    optional: PaymentOptionality::Optional,
                    on_paid: body?,
                    receipt,
                },
            },
        ))
    }

    fn bind_payment(
        &mut self,
        name: &str,
        receipt: ReceiptId,
        path: &str,
    ) -> Result<(), LoweringError> {
        if self
            .local_payments
            .insert(name.to_owned(), receipt)
            .is_some()
        {
            return Err(super::unsupported(path));
        }
        Ok(())
    }

    pub(super) fn optional_payment(
        &mut self,
        body: &ast::Body,
        source: &SourceSpan,
        origin: &AbilitySource,
        path: &str,
    ) -> Result<Option<PaymentDeclaration>, LoweringError> {
        if body.head_cost.is_some() && body.optional_cost.is_some() {
            return Err(super::unsupported(path));
        }
        let field = if body.optional_cost.is_some() {
            "optional_cost"
        } else {
            "cost"
        };
        let path = format!("{path}/{field}");
        let (play, optional, result) = if let Some(cost) = &body.optional_cost {
            (
                payment::lower_one(self, &cost.operation, origin, &path)?,
                PaymentOptionality::Optional,
                Some(&cost.result),
            )
        } else if let Some(cost) = &body.head_cost {
            (
                payment::lower(self, cost, origin, &path)?,
                PaymentOptionality::Required,
                match cost {
                    ast::Cost::Short(operation) => match operation.as_ref() {
                        ast::CostOperation::ToCemetery(material)
                        | ast::CostOperation::ReturnToHand(material)
                        | ast::CostOperation::Act(material) => material.result.as_ref(),
                        ast::CostOperation::ToEx(ast::ExMaterial::Public(material))
                        | ast::CostOperation::Banish(ast::BanishMaterial::Public(material)) => {
                            material.result.as_ref()
                        }
                        ast::CostOperation::ToEx(ast::ExMaterial::Hand(material))
                        | ast::CostOperation::Banish(ast::BanishMaterial::Hand(material)) => {
                            material.result.as_ref()
                        }
                        ast::CostOperation::Banish(ast::BanishMaterial::Top(_))
                        | ast::CostOperation::Pp(_)
                        | ast::CostOperation::Lesson(_)
                        | ast::CostOperation::Counter(_)
                        | ast::CostOperation::EarthRite(_)
                        | ast::CostOperation::LeaderLife(_)
                        | ast::CostOperation::Discard(_)
                        | ast::CostOperation::Reveal(_)
                        | ast::CostOperation::ToDeckBottom(_)
                        | ast::CostOperation::BanishSelf(_)
                        | ast::CostOperation::ActSelf(_)
                        | ast::CostOperation::SelfToCemetery(_)
                        | ast::CostOperation::SelfToEx(_) => None,
                    },
                    ast::Cost::Ordered { .. } => None,
                },
            )
        } else {
            return Ok(None);
        };
        let receipt = ReceiptId(self.card_allocator.id(&path)?);
        if let Some(result) = result {
            self.bind_payment(result, receipt, &path)?;
        }
        let mut located = source.clone();
        located.author_path.clone_from(&path);
        Ok(Some(PaymentDeclaration {
            node: Located {
                source: located,
                node: Node::Control {
                    control: Control::Pay {
                        at: PaymentTime::PlayParameter,
                        costs: play.costs,
                        optional,
                        on_paid: Program {
                            scope: self.card_allocator.scope(&path)?,
                            steps: vec![],
                        },
                        receipt,
                    },
                },
            },
            parameters: play.parameters,
        }))
    }
}

/// Moves a player-resource cost onto another payer; other costs belong to the source.
fn pay_as(costs: &mut CostSpec, payer: &PlayerRef, path: &str) -> Result<(), LoweringError> {
    let CostSpec::Ordered { steps } = costs else {
        return Err(super::unsupported(path));
    };
    for step in steps {
        let Action::Resource { player, .. } = &mut step.instruction.action else {
            return Err(super::unsupported(path));
        };
        player.clone_from(payer);
        step.instruction.actor.clone_from(payer);
    }
    Ok(())
}

pub(super) fn check_entry(
    entry: &ast::Ability,
    exchange: bool,
    path: &str,
) -> Result<(), LoweringError> {
    if entry.evolve.is_some()
        || entry.feed.is_some()
        || entry.ride.is_some()
        || entry.keywords.is_some()
    {
        return Ok(());
    }
    let (body, _, _) = super::ability_body(entry, path)?;
    if body.head_cost.is_some() && (exchange || entry.activate.is_some() || entry.spell.is_some()) {
        return Err(super::unsupported(path));
    }
    if body.optional_cost.is_some() && (exchange || entry.activate.is_some()) {
        return Err(super::unsupported(path));
    }
    Ok(())
}
