//! Components share the same authorized object scope rather than infer identity.

use super::ProjectionError;
use crate::game::player::{self, Viewer, observation as native};
use alloc::collections::BTreeSet;
use core::result;
use sve_battle_contract::ObjectHandle;

type Result<T> = result::Result<T, ProjectionError>;

pub(super) struct ObjectScope<'observation> {
    known: BTreeSet<&'observation player::ObjectHandle>,
    quotable: BTreeSet<&'observation player::ObjectHandle>,
}

impl<'observation> ObjectScope<'observation> {
    pub(super) fn new(observation: &'observation native::Observation) -> Result<Self> {
        if observation
            .players
            .iter()
            .map(|player| player.player)
            .ne([Viewer::P1, Viewer::P2])
        {
            return Err(ProjectionError::Unavailable);
        }
        let mut objects = Self {
            known: BTreeSet::new(),
            quotable: BTreeSet::new(),
        };
        for player in &observation.players {
            for zone in &player.zones {
                for member in &zone.cards {
                    let handle = &member.card.handle;
                    if !objects.known.insert(handle) {
                        return Err(ProjectionError::Unavailable);
                    }
                    if player.player == observation.viewer
                        && member.card.properties.controller == observation.viewer
                        && matches!(
                            zone.kind,
                            native::ObservedZone::Hand | native::ObservedZone::Ex
                        )
                    {
                        objects.quotable.insert(handle);
                    }
                }
            }
        }
        Ok(objects)
    }

    pub(super) fn quote(&self, handle: &player::ObjectHandle) -> Result<ObjectHandle> {
        if !self.quotable.contains(handle) {
            return Err(ProjectionError::Unavailable);
        }
        self.handle(handle)
    }

    pub(super) fn handle(&self, handle: &player::ObjectHandle) -> Result<ObjectHandle> {
        if !self.known.contains(handle) {
            return Err(ProjectionError::Unavailable);
        }
        Ok(ObjectHandle::new(handle.as_str().into()))
    }

    pub(super) fn optional(
        &self,
        handle: Option<&player::ObjectHandle>,
    ) -> Result<Option<ObjectHandle>> {
        handle.map(|handle| self.handle(handle)).transpose()
    }
}
