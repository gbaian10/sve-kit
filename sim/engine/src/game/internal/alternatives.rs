use super::{Execution, Game, InputDomain, Result, Wait, invalid, unsupported};
use crate::ir::{
    Cardinality, ChoiceDomain, Control, Expr, Mode, ModeChoiceKind, ModeDecision, ModeDecisionKind,
    ModeId, Nat, PlayerRef, Program, Visibility,
};

#[derive(Clone, Debug)]
pub(super) struct AlternativeWait {
    modes: Vec<Mode>,
    chosen: ModeDecision,
}

impl AlternativeWait {
    pub(super) fn validate(&self, answer: &[ModeId]) -> Result<Program> {
        if !self.chosen.accepts_answer(Nat(1), answer) {
            return Err(invalid("invalid resolution alternative"));
        }
        self.modes
            .iter()
            .find(|mode| answer.contains(&mode.id))
            .map(|mode| mode.body.clone())
            .ok_or_else(|| invalid("missing selected resolution alternative"))
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Resolution alternatives share the existing mode decision boundary."
)]
impl Game {
    pub(super) fn internal_alternative(
        &mut self,
        execution: &mut Execution,
        control: Control,
    ) -> Result<()> {
        let Control::Mode {
            modes,
            count: Expr::Literal(Nat(1)),
            chosen,
            choice_kind: ModeChoiceKind::ResolutionAlternative,
        } = control
        else {
            return Err(unsupported("one resolution alternative"));
        };
        let bounds = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
        let ChoiceDomain::Modes {
            modes: ids,
            cardinality,
            exclude_used: None,
            ..
        } = &chosen.domain
        else {
            return Err(unsupported("resolution alternative domain"));
        };
        if chosen.actor != (PlayerRef::Controller {})
            || chosen.visibility != (Visibility::Public {})
            || chosen.kind
                != (ModeDecisionKind::ResolutionChoice {
                    cardinality: bounds,
                })
            || *cardinality != bounds
            || modes.iter().any(|mode| !mode.parameters.is_empty())
        {
            return Err(unsupported("resolution alternative timing"));
        }
        let domain = InputDomain::Modes {
            modes: ids.clone(),
            cardinality: bounds,
            count: Nat(1),
        };
        let actor = execution.player(self, &chosen.actor)?;
        self.internal_wait(
            execution,
            chosen.id,
            actor,
            domain,
            Wait::Alternative(AlternativeWait { modes, chosen }),
        )
    }
}
