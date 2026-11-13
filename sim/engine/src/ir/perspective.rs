//! A player set is read from one perspective: the effect controller, or inside a player
//! procedure, that procedure's player. The effect controller and source never change.
use super::{Action, PlayerRef};

/// Whether `relative_to` names the effect controller or an enclosing procedure's player.
pub(crate) const fn readable(relative_to: &PlayerRef) -> bool {
    matches!(
        relative_to,
        PlayerRef::Controller {} | PlayerRef::Bound { .. }
    )
}

/// The player an action's structure names as the one performing it: the drawing, discarding,
/// searching or shuffling player, or the player whose whole zones return to the deck. Counts,
/// conditions and recipients elsewhere in the action are only read from their own players.
pub(crate) fn performer(action: &Action) -> Option<&PlayerRef> {
    if let Action::Discard { player, .. }
    | Action::Draw { player, .. }
    | Action::Search { player, .. }
    | Action::Shuffle { deck: player, .. } = action
    {
        Some(player)
    } else {
        super::shuffle::whole_return(action)
    }
}
