use super::{Execution, Game, Result, unsupported};
use crate::ir::{DurationEnd, Interval, ObjectOperand, ZoneKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum EffectEnd {
    NoTemporalExpiry,
    EndResolution,
    Turn(u64),
    NextTurn { player: u32, after: u64 },
    SourceField { object: u64, generation: u64 },
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
            DurationEnd::SourceZoneExit {
                source: ObjectOperand::SourceHolder {},
                zone: ZoneKind::Field,
            } => {
                let source = execution.object(game, &ObjectOperand::SourceHolder {})?;
                let object = game.object(execution.object_id(source)?)?;
                if object.zone != "field" || object.generation != source.generation.0 {
                    return Err(unsupported(
                        "source-zone duration requires current field source",
                    ));
                }
                Ok(Self::SourceField {
                    object: source.id.0,
                    generation: source.generation.0,
                })
            }
            _ => Err(unsupported("effect duration")),
        }
    }

    pub(super) const fn begin(&mut self, turn: u64, player: u32) {
        if matches!(*self, Self::NextTurn { player: waiting, after } if waiting == player && turn > after)
        {
            *self = Self::Turn(turn);
        }
    }

    pub(super) fn source_expired(self, game: &Game) -> bool {
        if let Self::SourceField { object, generation } = self {
            return !game
                .state
                .ir_objects
                .iter()
                .find(|(_, id)| **id == object)
                .and_then(|(id, _)| game.state.objects.get(id))
                .is_some_and(|source| source.zone == "field" && source.generation == generation);
        }
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum TurnInterval {
    Waiting { player: u32, after: u64 },
    Active(u64),
    Finished,
}

impl TurnInterval {
    pub(super) fn at(game: &Game, execution: &Execution, interval: &Interval) -> Result<Self> {
        let Interval::DuringNextTurn { player } = interval else {
            return Err(unsupported(
                "phase interval requires actual phase occurrences",
            ));
        };
        Ok(Self::Waiting {
            player: execution.player(game, player)?.seat,
            after: game.state.schedule.current()?,
        })
    }

    pub(super) const fn begin(&mut self, turn: u64, player: u32) {
        if matches!(*self, Self::Waiting { player: waiting, after } if waiting == player && turn > after)
        {
            *self = Self::Active(turn);
        }
    }

    pub(super) const fn finish(&mut self, turn: u64) {
        if matches!(*self, Self::Active(current) if current == turn) {
            *self = Self::Finished;
        }
    }

    pub(super) const fn active(self) -> bool {
        matches!(self, Self::Active(_))
    }
    pub(super) const fn pending(self) -> bool {
        !matches!(self, Self::Finished)
    }
}
