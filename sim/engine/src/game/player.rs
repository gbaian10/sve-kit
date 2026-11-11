//! Shared vocabulary for typed player input, observations and projected events.
//! These types do not yet provide the player submission or observation APIs.

use crate::ir::PlayerId;

/// An authenticated player perspective supplied by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Viewer {
    /// First player.
    P1,
    /// Second player.
    P2,
}

impl Viewer {
    /// The valid seat associated with this player perspective.
    #[must_use]
    pub const fn player(self) -> PlayerId {
        match self {
            Self::P1 => PlayerId { seat: 1 },
            Self::P2 => PlayerId { seat: 2 },
        }
    }
}

/// A viewer's opaque reference during one continuously identifiable period.
/// Issuance and revocation belong to the observation service; submissions must revalidate it.
///
/// ```
/// use sve_engine::game::player::ObjectHandle;
///
/// fn handle_text(handle: &ObjectHandle) -> &str {
///     handle.as_str()
/// }
/// ```
///
/// ```compile_fail,E0603
/// use sve_engine::game::player::ObjectHandle;
///
/// let forged = ObjectHandle("physical-object-id".into());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectHandle(pub(crate) String);

impl ObjectHandle {
    /// Opaque text for display or comparison, independent of authority identities.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// A question's rules purpose, independent of its answer shape or presentation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuestionPurpose {
    /// Parameters of a play, including a nested play during effect resolution.
    PlayParameter,
    /// A choice belonging to an effect's resolution.
    ResolutionChoice,
    /// A choice required by a rules procedure.
    RuleChoice,
    /// A fixed Quick window, including one whose only legal answer is Pass.
    QuickWindow,
    /// An active player's main-phase action window.
    MainAction,
}

/// Classification for the future read-only player input result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputKind {
    /// This viewer must answer the current question.
    Question,
    /// Another player must answer; only authorized public summaries may accompany it.
    Waiting,
    /// A normal rules result ended the game.
    Finished,
    /// Automatic progression stopped on an engine fault, without deciding a winner.
    EngineFault,
}

/// Classification for the future player submission result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SubmissionKind {
    /// Accepted input advanced the procedure, possibly returning to target selection.
    Applied,
    /// Invalid actor, token or answer preserved the original question.
    Rejected,
    /// An illegal play returned to its origin and invalidated its question.
    Reverted,
    /// The existing rules procedure cancelled a pending occurrence.
    Cancelled,
    /// An engine failure preserved its rollback scope and stopped player progression.
    /// A rolled-back unsupported operation or damaged state remains an engine fault.
    EngineFault,
}
