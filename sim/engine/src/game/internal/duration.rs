use super::{Execution, Game, Result, unsupported};
use crate::ir::DurationEnd;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum EffectEnd {
    NoTemporalExpiry,
    EndResolution,
    Turn(u64),
    NextTurn { player: u32, after: u64 },
}

impl EffectEnd {
    pub(super) fn at(game: &Game, execution: &Execution, end: &DurationEnd) -> Result<Self> {
        match end {
            DurationEnd::NoTemporalExpiry {} => Ok(Self::NoTemporalExpiry),
            DurationEnd::EndResolution {} => Ok(Self::EndResolution),
            DurationEnd::CurrentTurnEnd {} | DurationEnd::NextTurnEnd { .. }
                if game.state.flow["kind"] == "pregame" || game.state.game["ended"] == true =>
            {
                Err(unsupported("turn duration requires an actual running turn"))
            }
            DurationEnd::CurrentTurnEnd {} => Ok(Self::Turn(game.state.schedule.current()?)),
            DurationEnd::NextTurnEnd { player } => Ok(Self::NextTurn {
                player: execution.player(game, player)?.seat,
                after: game.state.schedule.current()?,
            }),
            _ => Err(unsupported("effect duration")),
        }
    }

    pub(super) const fn begin(&mut self, turn: u64, player: u32) {
        if matches!(*self, Self::NextTurn { player: waiting, after } if waiting == player && turn > after)
        {
            *self = Self::Turn(turn);
        }
    }
}
