use super::{BodyContext, LoweringError, ast, unsupported};
use crate::ir::{
    AbilityKind, AbilityPredicate, Action, ActionPattern, Binding, Bool, Effect, Expr,
    InstallationSubjects, InstallationTime, KeywordId, PermissionSubject, Polarity, RuleId,
    Selector,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Ability permissions share the body binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn no_activation(
        &mut self,
        installation: &ast::Apply,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::InstalledEffect::Cannot(ast::Cannot::PlayAbility(shape)) = &installation.effect
        else {
            return Err(unsupported(path));
        };
        if !matches!(installation.on, ast::Objects::This(ast::This::This))
            || !matches!(installation.until, Some(ast::Until::Turn))
        {
            return Err(unsupported(path));
        }
        let except = match shape.ability.except.as_deref() {
            None => vec![],
            Some([ast::EvolutionException::Evolution]) => vec![KeywordId::Evolution],
            _ => return Err(unsupported(path)),
        };
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let duration = super::numeric::duration(installation.until.as_ref(), path)?;
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Objects {
                objects: super::numeric::holder(),
                affected,
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::Permission {
                subject: PermissionSubject::Objects {
                    objects: Selector::Bound {
                        reference: affected,
                    },
                },
                action: ActionPattern::PlayAbility {
                    predicate: AbilityPredicate {
                        kind: Some(AbilityKind::Activated),
                        keyword: None,
                        except,
                        holder: None,
                        origin: None,
                    },
                },
                polarity: Polarity::Forbid,
                condition: Expr::Literal(Bool(true)),
                duration: duration.clone(),
                rule: RuleId(133),
            }),
            duration,
        })
    }
}
