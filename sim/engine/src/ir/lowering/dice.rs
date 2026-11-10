//! Dice bindings and fixed repeats preserve the ordinary instruction and control trees.
use super::{
    Ability, AbilityId, AbilityKind, AbilityQualification, AbilitySource, Action, Allocator,
    Binding, BodyContext, Bool, CardCoordinates, Expr, Located, LoweringError, Nat, NatRead, Node,
    Predicate, ScopeId, SourceSpan, ZoneKind, ability_source, ast, source_span, unsupported,
};
use crate::ir::{DiceOperation, IterationCheck, RepeatBody, RuleId, Termination};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Dice lowering shares the body's typed binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn dice(&mut self, value: &ast::Dice, path: &str) -> Result<Action, LoweringError> {
        if self.dice_results.contains_key(&value.bind) {
            return Err(unsupported(path));
        }
        if let Some(who) = &value.who {
            let ast::SelfPlayer::Controller = who;
        }
        let result = Binding::new(self.allocator.id(path)?, self.scope);
        self.dice_results.insert(value.bind.clone(), result);
        Ok(Action::Dice {
            operation: DiceOperation::Roll {},
            result,
        })
    }

    pub(super) fn fixed_repeat(
        &mut self,
        value: &ast::Repeat,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Node, LoweringError> {
        let parent = self.scope;
        let scope = self.card_allocator.scope(path)?;
        self.scope = self.card_allocator.scope(path)?;
        let previous = self.dice_results.clone();
        let body = self.steps(&value.steps, &format!("{path}/repeat/do"), source, origin)?;
        self.dice_results = previous;
        self.scope = parent;
        Ok(Node::Control {
            control: super::super::Control::Repeat {
                body: RepeatBody::Program { body },
                termination: Termination::Count {
                    count: Expr::Literal(Nat(value.times)),
                },
                check: IterationCheck::BeforeIteration,
                scope,
                receipts: vec![],
            },
        })
    }

    pub(super) fn dice_history(
        &mut self,
        value: &ast::DiceHistory,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let ast::DiceHistoryEvent::DiceRolled = value.event;
        let ast::SelfPlayer::Controller = value.who;
        let ast::ThisTurn::ThisTurn = value.window;
        let face = value
            .r#where
            .as_ref()
            .map(|filter| {
                let (comparison, face) = match filter.value {
                    ast::DiceThreshold::Eq(n) => (super::super::Comparison::Equal, n),
                    ast::DiceThreshold::Le(n) => (super::super::Comparison::LessEqual, n),
                    ast::DiceThreshold::Ge(n) => (super::super::Comparison::GreaterEqual, n),
                };
                if !(1..=6).contains(&face) {
                    return Err(unsupported(path));
                }
                Ok((comparison, Nat(face)))
            })
            .transpose()?;
        let event = Binding::new(self.allocator.id(path)?, self.scope);
        Ok(Expr::Compute(Box::new(Predicate::CompareNat {
            left: Expr::Live(Box::new(NatRead::HistoryCount {
                query: super::super::dice::query(event, face),
                count: super::super::HistoryCount::Facts,
            })),
            comparison: super::super::Comparison::GreaterEqual,
            right: Expr::Literal(Nat(1)),
        })))
    }
}

pub(super) fn reroll(
    _allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    let [ast::StaticEffect::Reroll(value)] = head.effects.as_slice() else {
        return Err(unsupported(path));
    };
    let ast::SelfPlayer::Controller = value.who;
    if value.times == 0
        || head.r#while.is_some()
        || head
            .zone
            .as_ref()
            .is_some_and(|zone| zone.single() != Some(ZoneKind::Field))
    {
        return Err(unsupported(path));
    }
    let source = source_span(key, path, entry)?;
    let origin = ability_source(&source, None);
    Ok(Located {
        source,
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations: vec![super::super::dice::permission(
                Nat(value.times),
                RuleId(scope.0),
            )],
        },
    })
}
