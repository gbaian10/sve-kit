use super::{BodyContext, LoweringError, ast, unsupported};
use crate::ir::{
    Action, ActionPattern, Binding, Bool, Duration, DurationEnd, Edge, Effect, Expr,
    InstallationSubjects, InstallationTime, PermissionSubject, Phase, PlayerRef, Polarity, RuleId,
    Selector, StandScope, TimeAnchor,
};

fn player(who: ast::StandPlayer) -> PlayerRef {
    match who {
        ast::StandPlayer::Controller => PlayerRef::Controller {},
        ast::StandPlayer::Opponent => super::super::events::timing::opponent(),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Posture permissions own their closed lowering shape."
)]
impl BodyContext<'_> {
    pub(super) fn no_stand(
        &mut self,
        installation: &ast::Apply,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::InstalledEffect::Cannot(restriction) = &installation.effect else {
            return Err(unsupported(path));
        };
        let Some(ast::Until::NextPhase { next_phase }) = &installation.until else {
            return Err(unsupported(path));
        };
        let ast::StandAction::Stand = restriction.what;
        let ast::Affected::Affected = restriction.on;
        let ast::StartPhase::Start = restriction.during.phase;
        let ast::RuleCause::Rule = restriction.during.cause;
        let ast::StartPhase::Start = next_phase.phase;
        let ast::EndEdge::End = next_phase.edge;
        if restriction.during.who != next_phase.who {
            return Err(unsupported(path));
        }
        let objects = self.objects(&installation.on, path)?;
        let affected = Binding::new(1, self.card_allocator.scope(path)?);
        let targets = Selector::Bound {
            reference: affected,
        };
        let duration = Duration {
            end: DurationEnd::NextPhaseEdge {
                player: player(next_phase.who),
                phase: Phase::Start,
                edge: Edge::End,
            },
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        };
        Ok(Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::Permission {
                subject: PermissionSubject::Objects {
                    objects: targets.clone(),
                },
                action: ActionPattern::Stand {
                    objects: targets,
                    scope: StandScope::RuleStart {
                        player: player(restriction.during.who),
                    },
                },
                rule: RuleId(723),
                polarity: Polarity::Forbid,
                condition: Expr::Literal(Bool(true)),
                duration: duration.clone(),
            }),
            duration,
        })
    }
}
