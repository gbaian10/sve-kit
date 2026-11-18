//! Queue markers delimit actual stages and iterations, including nested procedures.

use super::super::{Result, invalid};
use crate::game::internal::fact_log::viewer;
use crate::game::player::Viewer;
use crate::game::player::observation::{ProcedureKind, ProcessingProcedure};
use crate::ir::{PlayerId, ScopeId};
use alloc::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Position {
    key: (ScopeId, u64),
    kind: ProcedureKind,
    actor: Option<PlayerId>,
    round: u64,
}

#[derive(Clone, Debug, Default)]
pub(in crate::game::internal) struct Procedures {
    active: Vec<Position>,
    repeats: BTreeMap<ScopeId, u64>,
}

impl Procedures {
    pub(in crate::game::internal) fn begin_players(&mut self, key: (ScopeId, u64)) {
        self.active.push(Position {
            key,
            kind: ProcedureKind::EachPlayer,
            actor: None,
            round: 0,
        });
    }

    pub(in crate::game::internal) fn players(&mut self, key: (ScopeId, u64), actor: PlayerId) {
        if let Some(position) = self
            .active
            .iter_mut()
            .rev()
            .find(|position| position.key == key)
        {
            position.actor = Some(actor);
        }
    }

    pub(in crate::game::internal) fn round(
        &mut self,
        key: (ScopeId, u64),
        round: u64,
    ) -> Result<()> {
        let position = self
            .active
            .last_mut()
            .filter(|position| position.key == key)
            .ok_or_else(|| invalid("processing procedure stage"))?;
        position.round = round;
        Ok(())
    }

    pub(in crate::game::internal) fn begin_repeat(&mut self, scope: ScopeId) -> Result<()> {
        let round = self.repeats.entry(scope).or_default();
        *round = round
            .checked_add(1)
            .ok_or_else(|| invalid("processing repeat round overflow"))?;
        self.active.push(Position {
            key: (scope, 0),
            kind: ProcedureKind::Repeat,
            actor: None,
            round: *round,
        });
        Ok(())
    }

    pub(in crate::game::internal) fn finish_repeat(&mut self, scope: ScopeId) {
        self.repeats.remove(&scope);
    }

    pub(in crate::game::internal) fn end(&mut self, key: (ScopeId, u64)) -> Result<()> {
        if self
            .active
            .last()
            .is_none_or(|position| position.key != key)
        {
            return Err(invalid("processing procedure end"));
        }
        self.active.pop();
        Ok(())
    }

    pub(super) fn observed(&self, answering: Viewer) -> Option<ProcessingProcedure> {
        let position = self.active.last()?;
        Some(ProcessingProcedure {
            kind: position.kind,
            actor: match position.actor {
                Some(actor) => viewer(actor)?,
                None => answering,
            },
            round: position.round,
        })
    }
}
