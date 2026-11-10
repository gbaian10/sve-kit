use super::{LoweringError, ast, numeric};
use crate::ir::{
    ActionPattern, Bool, Cardinality, Effect, Expr, Nat, PermissionSubject, PlayerRef,
    PlayerRelation, PlayerSet, Polarity, RuleId, TextQuantifier,
};

pub(super) fn permission(shape: &ast::CannotPlayer, path: &str) -> Result<Effect, LoweringError> {
    let players = PlayerSet {
        relation: if matches!(shape.who, ast::EntryPlayer::Controller) {
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
    };
    Ok(Effect::Permission {
        subject: PermissionSubject::Players {
            players: players.clone(),
        },
        action: match shape.what {
            ast::ResultAction::Win => ActionPattern::Win { players },
            ast::ResultAction::Lose => ActionPattern::Lose { players },
        },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: numeric::duration(None, path)?,
        rule: RuleId(133),
    })
}
