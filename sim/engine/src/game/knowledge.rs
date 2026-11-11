//! Location changes own handle issuance and revocation. Authority questions and events
//! retain physical references; only their viewer projections contain handle strings.

use super::Game;
use super::player::observation::{seat, viewer};
use super::player::{ObjectHandle, Viewer};
use crate::random::Random;
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt::Write as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Knowledge {
    pub(super) seen: BTreeMap<String, Value>,
    located: BTreeSet<String>,
    pub(super) carried: Vec<Value>,
    handles: BTreeMap<String, HandleEntry>,
    issued: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HandleEntry {
    value: String,
    index: u64,
}

#[derive(Debug, Clone)]
struct HandleSnapshot {
    issued: u64,
    handles: BTreeMap<String, HandleEntry>,
}

/// Call-local knowledge boundary; never stored in an execution or event.
#[derive(Debug)]
pub(in crate::game) struct PublishedKnowledge(BTreeMap<String, HandleSnapshot>);

impl Knowledge {
    pub(super) const fn located(&self) -> &BTreeSet<String> {
        &self.located
    }

    pub(super) fn knows_location(&self, id: &str) -> bool {
        self.located.contains(id)
    }

    pub(super) fn identity_period(&self, id: &str) -> Option<u64> {
        self.handles.get(id).map(|entry| entry.index)
    }

    pub(super) fn handle(&self, id: &str) -> Option<ObjectHandle> {
        self.handles
            .get(id)
            .map(|entry| ObjectHandle(entry.value.clone()))
    }

    pub(super) fn locate(&mut self, id: &str, key: &Random) -> Result<()> {
        if !self.located.contains(id) {
            let next = self
                .issued
                .checked_add(1)
                .ok_or_else(|| invalid("player object handles exhausted"))?;
            let mut text = String::from("vobj:");
            for byte in key.opaque_at(self.issued) {
                write!(text, "{byte:02x}").map_err(invalid)?;
            }
            self.handles.insert(
                id.into(),
                HandleEntry {
                    value: text,
                    index: self.issued,
                },
            );
            self.issued = next;
            self.located.insert(id.into());
        }
        Ok(())
    }

    pub(super) fn forget_location(&mut self, id: &str) {
        self.located.remove(id);
        self.handles.remove(id);
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Knowledge owns player identity periods."
)]
impl Game {
    pub(super) fn learn(&mut self, seat: &str, id: &str, located: bool) -> Result<()> {
        self.learn_members(seat, &[id.into()], located)
    }

    pub(super) fn learn_members(
        &mut self,
        seat: &str,
        ids: &[String],
        located: bool,
    ) -> Result<()> {
        let perspective = viewer(seat)?;
        let mut members = ids
            .iter()
            .filter(|id| self.state.objects.contains_key(id.as_str()))
            .map(|id| {
                self.location_key(perspective, id)
                    .map(|key| (key, id.clone()))
            })
            .collect::<Result<Vec<_>>>()?;
        members.sort_by(|left, right| left.0.cmp(&right.0));
        let key = self
            .state
            .rng
            .fork(&format!("player-object-handles/{seat}"));
        for (_, id) in members {
            let object = self.object(&id)?;
            let record = json!({"id":id,"card":object.card,"owner":object.owner,"from":self.node});
            let knowledge = self.state.knowledge.entry(seat.into()).or_default();
            knowledge.seen.entry(id.clone()).or_insert(record);
            if located {
                knowledge.locate(&id, &key)?;
            }
        }
        Ok(())
    }

    fn location_key(&self, perspective: Viewer, id: &str) -> Result<(String, Option<u64>, String)> {
        let object = self.object(id)?;
        let mut position = None;
        if object.zone != "hand" || object.controller == seat(perspective) {
            let mut offset = 0_u64;
            for item in self
                .player(&object.controller)?
                .zones
                .get(&object.zone)
                .into_iter()
                .flatten()
            {
                if item.as_str() == Some(id) {
                    position = Some(offset);
                    break;
                }
                offset = offset
                    .checked_add(if item.is_string() {
                        1
                    } else {
                        item["filler"]
                            .as_u64()
                            .ok_or_else(|| invalid("invalid anonymous zone member"))?
                    })
                    .ok_or_else(|| invalid("zone position overflow"))?;
            }
        }
        let source = self.information_source(id)?;
        let visible = json!({"card": object.card, "face":source.state["face"],
            "owner": object.owner, "controller":object.controller});
        Ok((
            format!("{}/{}", object.controller, object.zone),
            position,
            serde_json::to_string(&visible).map_err(invalid)?,
        ))
    }

    /// Resolve only a current viewer capability; consumers still validate their frozen domain.
    pub(in crate::game) fn resolve_player_object(
        &self,
        perspective: Viewer,
        handle: &ObjectHandle,
    ) -> Result<&str> {
        let knowledge = self
            .state
            .knowledge
            .get(seat(perspective))
            .ok_or_else(|| invalid("invalid player object handle"))?;
        knowledge
            .handles
            .iter()
            .find_map(|(id, value)| {
                (value.value == handle.as_str() && self.player_knows_object(perspective, id))
                    .then_some(id.as_str())
            })
            .ok_or_else(|| invalid("invalid player object handle"))
    }

    pub(in crate::game) fn published_knowledge(&self) -> PublishedKnowledge {
        PublishedKnowledge(
            self.state
                .knowledge
                .iter()
                .map(|(seat, known)| {
                    (
                        seat.clone(),
                        HandleSnapshot {
                            issued: known.issued,
                            handles: known.handles.clone(),
                        },
                    )
                })
                .collect(),
        )
    }

    pub(in crate::game) fn restore_player_state(
        &mut self,
        mut target: Self,
        published: &PublishedKnowledge,
    ) -> Result<()> {
        target.state.ir_epoch = self.state.ir_epoch.max(target.state.ir_epoch);
        for (seat, previous) in &published.0 {
            let Some(knowledge) = target.state.knowledge.get_mut(seat) else {
                continue;
            };
            knowledge.issued = knowledge.issued.max(previous.issued);
            {
                let interrupted = knowledge
                    .located
                    .iter()
                    .filter(|id| {
                        knowledge.handles.get(*id).is_none_or(|entry| {
                            entry.index < previous.issued
                                && Some(entry) != previous.handles.get(*id)
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                for id in &interrupted {
                    knowledge.forget_location(id);
                }
                target.learn_members(seat, &interrupted, true)?;
            }
        }
        *self = target;
        Ok(())
    }
}
