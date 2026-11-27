//! Dice bindings and fixed repeats preserve the ordinary instruction and control trees.
use super::{
    Ability, AbilityId, AbilityKind, AbilityQualification, AbilitySource, Action, Allocator,
    Binding, BodyContext, Bool, CardCoordinates, Expr, Located, LoweringError, Nat, NatRead, Node,
    PlayerRef, Predicate, ScopeId, SourceSpan, ZoneKind, ability_source, ast, source_span,
    unsupported,
};
use crate::ir::{DiceOperation, IterationCheck, RepeatBody, RuleId, Termination};

impl BodyContext<'_> {
    pub(super) fn dice(&mut self, value: &ast::Dice, path: &str) -> Result<Action, LoweringError> {
        if self.dice_results.contains_key(&value.bind) {
            return Err(unsupported(path));
        }
        let result = Binding::new(self.allocator.id(path)?, self.scope);
        self.dice_results.insert(value.bind.clone(), result);
        Ok(Action::Dice {
            operation: DiceOperation::Roll {},
            result,
        })
    }

    pub(super) fn dice_actor(
        &self,
        value: &ast::Dice,
        path: &str,
    ) -> Result<PlayerRef, LoweringError> {
        match &value.who {
            None | Some(ast::Side::Controller) => Ok(PlayerRef::Controller {}),
            Some(ast::Side::Bound(name)) => self.named_effect_player(name, path),
            Some(ast::Side::Opponent) => self.named_effect_player("opp", path),
            Some(ast::Side::All) => Err(unsupported(path)),
        }
    }

    pub(super) fn fixed_repeat(
        &mut self,
        value: &ast::Repeat,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Node, LoweringError> {
        if value.until.is_some() || value.r#while.is_some() || value.collect.is_some() {
            return self.repeated_move(value, path, source, origin);
        }
        if value.check.is_some() {
            return Err(unsupported(path));
        }
        let count = value.times.ok_or_else(|| unsupported(path))?;
        let parent = self.scope;
        let scope = self.card_allocator.scope(path)?;
        self.scope = self.card_allocator.scope(path)?;
        let saved = self.local_bindings();
        let body = self.steps(&value.steps, &format!("{path}/repeat/do"), source, origin);
        self.restore_bindings(saved);
        self.scope = parent;
        Ok(Node::Control {
            control: super::super::Control::Repeat {
                body: RepeatBody::Program { body: body? },
                termination: Termination::Count {
                    count: Expr::Literal(Nat(count)),
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
