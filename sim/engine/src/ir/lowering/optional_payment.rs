use super::{
    AbilitySource, BodyContext, KeywordId, Located, LoweringError, Node, SourceSpan, ast, payment,
};
use crate::ir::{Control, Decision, PaymentOptionality, PaymentTime, Program, ReceiptId};

pub(super) struct PaymentDeclaration {
    pub node: Located<Node>,
    pub parameters: Vec<Decision>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Optional payment shares the body binding environment."
)]
impl BodyContext<'_> {
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
    if entry.evolve.is_some() || entry.keywords.is_some() {
        return Ok(());
    }
    let (body, _, keyword) = super::ability_body(entry, path)?;
    if body.head_cost.is_some()
        && (exchange
            || entry.activate.is_some()
            || entry.spell.is_some()
            || (!entry.auto.as_ref().is_some_and(ast::Auto::cemetery_play)
                && !matches!(
                    keyword,
                    Some(
                        KeywordId::Fanfare
                            | KeywordId::Lastword
                            | KeywordId::OnEvolve
                            | KeywordId::OnSuperEvolve
                            | KeywordId::OnAttack
                            | KeywordId::OnRace
                            | KeywordId::OnDriveGain
                    )
                )))
    {
        return Err(super::unsupported(path));
    }
    if body.optional_cost.is_some()
        && (exchange
            || entry.activate.is_some()
            || (entry.spell.is_none() && keyword != Some(KeywordId::Fanfare)))
    {
        return Err(super::unsupported(path));
    }
    Ok(())
}
