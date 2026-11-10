use super::{Action, Binding, BodyContext, Expr, Int, LoweringError, Nat, ast, unsupported};
use crate::ir::{ObjectReceipt, ReceiptId, ReceiptReference, RevealUntilMembers, reveal_until};

pub(super) fn lower(
    context: &mut BodyContext<'_>,
    value: &ast::RevealUntil,
    path: &str,
) -> Result<Action, LoweringError> {
    let ast::SelfPlayer::Controller = value.sel.who;
    let ast::DeckZone::Deck = value.sel.zone;
    let ast::RevealFollower::Follower = value.sel.kind;
    if value.n == 0 {
        return Err(unsupported(path));
    }
    let maximum = i64::try_from(value.sel.orig_cost_le).map_err(|_overflow| unsupported(path))?;
    let candidate = Binding::new(context.allocator.id(path)?, context.scope);
    Ok(Action::RevealUntil {
        deck: super::PlayerRef::Controller {},
        candidate,
        predicate: reveal_until::filter(candidate, Int(maximum)),
        required: Expr::Literal(Nat(value.n)),
    })
}

pub(super) fn remember(
    context: &mut BodyContext<'_>,
    value: &ast::RevealUntil,
    receipt: ReceiptId,
    path: &str,
) -> Result<(), LoweringError> {
    for (name, members) in [
        (&value.bind, RevealUntilMembers::Revealed),
        (&value.matched, RevealUntilMembers::Matched),
    ] {
        if name.is_empty()
            || context.targets.contains_key(name)
            || context.observations.contains_key(name)
            || context.searches.contains(name)
            || context.receipt_reads.evolutions.contains_key(name)
        {
            return Err(unsupported(path));
        }
        context.observations.insert(
            name.clone(),
            ObjectReceipt::RevealUntil {
                receipt: ReceiptReference::new(receipt),
                members,
            },
        );
    }
    Ok(())
}
