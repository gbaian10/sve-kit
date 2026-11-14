//! Thin neutral-runner adapter; it only converts input and result types.

mod typed;
pub use typed::TypedAdapter;

use sve_scenario_runner::{EngineError, View};

use crate::game;

pub(crate) fn error(error: crate::EngineFailure) -> EngineError {
    match error {
        crate::EngineFailure::Unsupported(reason) => EngineError::Unsupported(reason),
        crate::EngineFailure::Invalid(reason)
        | crate::EngineFailure::IllegalPlay(reason)
        | crate::EngineFailure::AuthorizationIntegrity(reason)
        | crate::EngineFailure::Invariant(reason) => EngineError::Adapter(reason),
    }
}
pub(crate) const fn core_view(view: View) -> game::View {
    match view {
        View::Omniscient => game::View::Referee,
        View::P1 => game::View::P1,
        View::P2 => game::View::P2,
    }
}

#[cfg(test)]
mod tests {
    use super::error;
    use crate::EngineFailure;
    use sve_scenario_runner::EngineError;

    #[test]
    fn only_capability_gaps_are_reported_as_unsupported_to_the_runner() {
        let reason = "synthetic diagnostic";
        assert!(matches!(
            error(EngineFailure::Unsupported(reason.into())),
            EngineError::Unsupported(message) if message == reason
        ));
        for failure in [
            EngineFailure::Invalid(reason.into()),
            EngineFailure::IllegalPlay(reason.into()),
            EngineFailure::AuthorizationIntegrity(reason.into()),
            EngineFailure::Invariant(reason.into()),
        ] {
            assert!(matches!(error(failure), EngineError::Adapter(message) if message == reason));
        }
    }
}
