use super::{
    Ability, Allocator, BodyContext, CardCoordinates, Located, LoweringError, ScopeId,
    ability_source, ast, source_span, unsupported,
};
use crate::ir::{Binding, Bool, Expr, PlayerRef, Selector, ZoneKind, spell_chain};
use core::slice::from_ref;

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let [ast::StaticEffect::RuleOverride(ast::RuleOverride::SpellChainPopulation(rule))] =
        head.effects.as_slice()
    else {
        return Err(unsupported(path));
    };
    if head
        .zone
        .as_ref()
        .is_some_and(|zones| zones.single() != Some(ZoneKind::Field))
    {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    let empty: ast::Body = super::decode(serde_json::json!({"do":[]}), path)?;
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    let ast::SelfPlayer::Controller = rule.who;
    let queries = match &rule.of {
        ast::Population::Query(query) => from_ref(query.as_ref()),
        ast::Population::Union { any_of } => any_of.as_slice(),
    };
    let mut sets = Vec::new();
    for query in queries {
        let binding = Binding::new(context.card_allocator.id(path)?, scope);
        sets.push(super::costs::public_query(query, binding, path)?);
    }
    let population = Selector::Union { sets };
    let condition = head
        .r#while
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?
        .unwrap_or(Expr::Literal(Bool(true)));
    Ok(Located {
        node: spell_chain::ability(
            ability_source(&source, None),
            scope,
            PlayerRef::Controller {},
            population,
            condition,
        ),
        source,
    })
}

impl BodyContext<'_> {
    pub(super) fn spell_chain_condition(
        &mut self,
        threshold: u64,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        use crate::ir::{
            Capture, CaptureRef, CaptureTime, Comparison, ExecutionRef, Int, IntOperation,
            NatFrozen, Predicate, SpellCountTime,
        };
        let into = CaptureRef {
            binding: Binding::new(self.allocator.id(path)?, self.resolution_scope),
        };
        self.resolution_captures.push(Capture::Integer {
            at: CaptureTime::AtResolution {
                scope: self.resolution_scope,
            },
            value: Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Frozen(Box::new(NatFrozen::SpellChain {
                    player: PlayerRef::Controller {},
                    execution: ExecutionRef::Current {},
                    at: SpellCountTime::AtResolution,
                })),
            })),
            into,
        });
        Ok(Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Captured(into),
            comparison: Comparison::GreaterEqual,
            right: Expr::Literal(Int(
                i64::try_from(threshold).map_err(|_private| unsupported(path))?
            )),
        })))
    }
}
