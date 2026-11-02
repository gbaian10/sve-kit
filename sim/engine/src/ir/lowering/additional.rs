use super::{Allocator, BodyContext, CardCoordinates, Located, LoweringError, ast};
use crate::ir::{PlayCostPayment, ReceiptId};

pub(super) fn lower(
    entry: &ast::Ability,
    play_costs: &ast::PlayCosts,
    key: &CardCoordinates<'_>,
    entry_path: &str,
    allocator: &mut Allocator,
) -> Result<Option<Located<PlayCostPayment>>, LoweringError> {
    let Some(cost) = &play_costs.additional_cost else {
        return Ok(None);
    };
    let mut source = super::source_span(key, entry_path, entry)?;
    source.author_path = format!("{entry_path}/play/additional_cost");
    let path = &source.author_path;
    let origin = super::ability_source(&source, None);
    let scope = allocator.scope(path)?;
    let body = ast::Body::default();
    let mut context = BodyContext::new(entry, scope, &body, None, allocator);
    let play = super::payment::lower_additional(&mut context, &cost.cost, &origin, path)?;
    let receipt = ReceiptId(context.card_allocator.id(path)?);
    Ok(Some(Located {
        source,
        node: PlayCostPayment {
            scope,
            play,
            receipt,
        },
    }))
}
