use super::{AbilitySource, BodyContext, Located, LoweringError, Node, SourceSpan, ast, payment};
use crate::ir::{Control, PaymentOptionality, PaymentTime, Program, ReceiptId};

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
    ) -> Result<Option<Located<Node>>, LoweringError> {
        let Some(cost) = &body.optional_cost else {
            return Ok(None);
        };
        let path = format!("{path}/optional_cost");
        let costs = payment::lower(
            self.card_allocator,
            &ast::Cost::Short(ast::CostOperation::Pp(cost.pp)),
            origin,
            &path,
        )?
        .costs;
        let receipt = ReceiptId(self.card_allocator.id(&path)?);
        self.local_payment = Some((cost.result.clone(), receipt));
        let mut located = source.clone();
        located.author_path.clone_from(&path);
        Ok(Some(Located {
            source: located,
            node: Node::Control {
                control: Control::Pay {
                    at: PaymentTime::PlayParameter,
                    costs,
                    optional: PaymentOptionality::Optional,
                    on_paid: Program {
                        scope: self.card_allocator.scope(&path)?,
                        steps: vec![],
                    },
                    receipt,
                },
            },
        }))
    }
}
