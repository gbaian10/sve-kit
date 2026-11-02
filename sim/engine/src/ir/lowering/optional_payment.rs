use super::{AbilitySource, BodyContext, Located, LoweringError, Node, SourceSpan, ast, payment};
use crate::ir::{Control, Decision, PaymentOptionality, PaymentTime, Program, ReceiptId};
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
        let operations = match &payment.cost {
            ast::Cost::Short(operation) => slice::from_ref(operation.as_ref()),
            ast::Cost::Ordered { steps } => steps.as_slice(),
        };
        if operations.is_empty()
            || operations.iter().any(|operation| {
                !matches!(
                    operation,
                    ast::CostOperation::ActSelf(true) | ast::CostOperation::SelfToCemetery(true)
                )
            })
        {
            return Err(super::unsupported(path));
        }
        let play = payment::lower(self, &payment.cost, origin, path)?;
        let scope = self.scope;
        self.scope = self.card_allocator.scope(path)?;
        let body = self.steps(&payment.steps, path, source, origin);
        self.scope = scope;
        let receipt = ReceiptId(self.card_allocator.id(path)?);
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
                None,
            )
        } else {
            return Ok(None);
        };
        let receipt = ReceiptId(self.card_allocator.id(&path)?);
        if let Some(result) = result {
            self.local_payment = Some((result.clone(), receipt));
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

pub(super) fn check_entry(
    entry: &ast::Ability,
    exchange: bool,
    path: &str,
) -> Result<(), LoweringError> {
    if entry.evolve.is_some() || entry.feed.is_some() || entry.keywords.is_some() {
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
