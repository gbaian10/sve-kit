//! Source information belongs to the checked ability execution, never to a caller's snapshot.

use super::{Execution, Game, Result, Slot, invalid, unsupported};
use crate::ir::{
    CardName, CharacteristicView, CounterKind, ExecutionId, Expr, LkiReason, NameOperation, Nat,
    NumberRead, ObjectRef, ObjectView, Requirement, Requirements, RuntimeAbilitySource,
    RuntimeObjectView, SnapshotId, Stat,
};
use alloc::collections::BTreeMap;
use serde_json::Value;

#[derive(Clone, Debug)]
pub(super) struct SourceInformation {
    holder: ObjectRef,
    zone: String,
    departed: Option<SourceSnapshot>,
}

#[derive(Clone, Debug)]
pub(super) struct SourceSnapshot {
    view: RuntimeObjectView,
    original: [i64; 3],
    effective: [i64; 3],
    names: Vec<CardName>,
    counters: BTreeMap<String, Nat>,
}

#[derive(Clone, Debug)]
pub(super) struct EventObject {
    pub(super) expression: Expr<ObjectRef>,
    pub(super) reference: ObjectRef,
    pub(super) information: Option<SourceSnapshot>,
}

/// Original is a private read elsewhere; only a checked source's own number read is exempt.
pub(super) fn source_original(requirements: &Requirements, requirement: &Requirement) -> bool {
    let at = |code: &str, path: &str| {
        requirements.required.iter().any(|other| {
            other.capability.code == code
                && other
                    .locations
                    .iter()
                    .any(|location| location.ir_path == path)
        })
    };
    requirement.capability.code == "internal.CharacteristicView.Original"
        && requirement.locations.iter().all(|location| {
            location.ir_path.strip_suffix("/view").is_some_and(|read| {
                (at("read.cost", read) || at("read.stats", read))
                    && at("reference.source-information", &format!("{read}/object"))
            })
        })
}

// A scalar count does not grant authority to select hidden objects in other contexts.
pub(super) fn count_zone(requirements: &Requirements, requirement: &Requirement) -> bool {
    matches!(
        requirement.capability.code,
        "internal.ZoneKind.Hand" | "internal.ZoneKind.Deck"
    ) && requirement.locations.iter().all(|location| {
        location
            .ir_path
            .strip_suffix("/selector/zone")
            .is_some_and(|read| {
                requirements.required.iter().any(|other| {
                    other.capability.code == "read.zone-count"
                        && other
                            .locations
                            .iter()
                            .any(|count_location| count_location.ir_path == read)
                })
            })
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Source reads share the execution's private authority proof."
)]
impl Execution {
    /// Binds the authority-derived ability source and its reference zone at body start.
    pub(super) fn bind_source(&mut self, game: &Game, source: RuntimeAbilitySource) -> Result<()> {
        self.ability = Some(source);
        self.source_information = Some(SourceInformation::current(game, source)?);
        Ok(())
    }

    fn source_snapshot(&self, game: &Game) -> Result<SourceSnapshot> {
        self.source_information
            .as_ref()
            .ok_or_else(|| invalid("missing ability source information producer"))?
            .snapshot(
                game,
                self.ability
                    .ok_or_else(|| invalid("missing ability source"))?,
            )
    }

    pub(super) fn source_counter(&self, game: &Game, kind: &CounterKind) -> Result<Nat> {
        let snapshot = self.source_snapshot(game)?;
        let key = game.catalog.keyword_id(&kind.0);
        Ok(snapshot.counters.get(&key).copied().unwrap_or(Nat(0)))
    }

    pub(super) fn name(&self, game: &Game, value: &Expr<CardName>) -> Result<CardName> {
        match value {
            Expr::Literal(name) => Ok(name.clone()),
            Expr::Captured(reference) => match self.slots.get(&reference.binding.key()) {
                Some(Slot::Name(name)) => Ok(name.clone()),
                _ => Err(invalid("missing captured name")),
            },
            Expr::Compute(operation) => {
                let NameOperation::UniqueName { object } = operation.as_ref();
                let names = match object {
                    ObjectView::Selected { selection } => {
                        let reference = self.selected_object(game, *selection)?;
                        let id = self.object_id(reference)?;
                        if game.object(id)?.generation != reference.generation.0 {
                            return Err(invalid("stale selected name"));
                        }
                        game.card_names(id)?
                            .into_iter()
                            .map(CardName)
                            .collect::<Vec<_>>()
                    }
                    ObjectView::LastKnown {
                        object,
                        reason: LkiReason::ZoneExit,
                    } => {
                        let reference = self.object(game, object)?;
                        let information = self
                            .event_object
                            .as_ref()
                            .filter(|subject| subject.reference == reference)
                            .and_then(|subject| subject.information.as_ref())
                            .ok_or_else(|| invalid("missing triggering subject information"))?;
                        if information.view.object != reference {
                            return Err(invalid("foreign triggering subject information"));
                        }
                        information.names.clone()
                    }
                    _ => return Err(unsupported("name information view")),
                };
                match names.as_slice() {
                    [name] => Ok(name.clone()),
                    _ => Err(unsupported("a copied token requires one name")),
                }
            }
            _ => Err(unsupported("name value")),
        }
    }

    pub(super) fn source_names(&self, game: &Game) -> Result<Vec<CardName>> {
        self.source_snapshot(game).map(|snapshot| snapshot.names)
    }

    pub(super) fn source_before_move(
        &self,
        game: &Game,
        ids: &[String],
    ) -> Result<Vec<(ExecutionId, SourceSnapshot)>> {
        let mut snapshots = Vec::new();
        if self.source_information.as_ref().is_some_and(|information| {
            information.departed.is_none() && ids.iter().any(|id| id == &self.context.source)
        }) {
            snapshots.push((self.id, self.source_snapshot(game)?));
        }
        // A child can move a suspended parent's source before that parent resumes.
        if let Some(nested) = &self.nested {
            snapshots.extend(nested.parent.source_before_move(game, ids)?);
        }
        Ok(snapshots)
    }

    pub(super) fn seal_source_departure(&mut self, snapshots: &[(ExecutionId, SourceSnapshot)]) {
        if let Some(information) = &mut self.source_information {
            information.seal(
                snapshots
                    .iter()
                    .find(|(id, _)| *id == self.id)
                    .map(|(_, snapshot)| snapshot.clone()),
            );
        }
        if let Some(nested) = &mut self.nested {
            nested.parent.seal_source_departure(snapshots);
        }
    }

    pub(super) fn restore_source_liveness(&mut self, game: &Game) {
        if let Some(information) = &mut self.source_information
            && game.object(&self.context.source).is_ok_and(|object| {
                object.generation == information.holder.generation.0
                    && object.zone == information.zone
            })
        {
            // Child rollback can restore the exact generation that a tentative move sealed.
            information.departed = None;
        }
        if let Some(nested) = &mut self.nested {
            nested.parent.restore_source_liveness(game);
        }
    }

    pub(super) fn number_information(&self, game: &Game, read: &NumberRead) -> Result<i64> {
        let (object, view, index) = match read {
            NumberRead::Cost { object, view } => (object, view, 0),
            NumberRead::Stats {
                object,
                view,
                property,
            } => (
                object,
                view,
                match property {
                    Stat::Attack => 1,
                    Stat::Life => 2,
                },
            ),
        };
        let current = match object {
            ObjectView::Current { object } => Some(self.object(game, object)?),
            ObjectView::Selected { selection } => Some(self.selected_object(game, *selection)?),
            _ => None,
        };
        if let Some(reference) = current {
            let id = self.object_id(reference)?;
            if game.object(id)?.generation != reference.generation.0 {
                return Err(invalid("stale Current information read"));
            }
            // Original cost does not require the target's unrelated abilities or numeric effects.
            if matches!(object, ObjectView::Selected { .. })
                && *view == CharacteristicView::Original
                && matches!(read, NumberRead::Cost { .. })
            {
                return super::cost::printed_cost(&game.internal_cost_print(id)?["cost"]);
            }
            if *view != CharacteristicView::Effective {
                return Err(unsupported("number characteristic view"));
            }
            return match read {
                NumberRead::Cost { .. } => game.internal_effective_cost(id),
                NumberRead::Stats { property, .. } => game.internal_numeric_stat(id, *property),
            };
        }
        let snapshot = match object {
            ObjectView::SourceInformation {} => self.source_snapshot(game)?,
            _ => return Err(unsupported("number information view")),
        };
        let values = match view {
            CharacteristicView::Original => snapshot.original,
            CharacteristicView::Effective => snapshot.effective,
            _ => return Err(unsupported("number characteristic view")),
        };
        values
            .get(index)
            .copied()
            .ok_or_else(|| invalid("missing numeric characteristic"))
    }

    pub(super) fn damage_source(
        &self,
        game: &Game,
        view: &ObjectView,
    ) -> Result<RuntimeObjectView> {
        match view {
            ObjectView::SourceInformation {} => {
                self.source_snapshot(game).map(|snapshot| snapshot.view)
            }
            ObjectView::Current { object } => {
                let reference = self.object(game, object)?;
                if game.object(self.object_id(reference)?)?.generation != reference.generation.0 {
                    return Err(invalid("damage source generation is no longer current"));
                }
                Ok(RuntimeObjectView {
                    object: reference,
                    snapshot: SnapshotId(game.state.ir_epoch),
                })
            }
            _ => Err(unsupported("damage source view")),
        }
    }
}

impl SourceInformation {
    pub(super) fn current(game: &Game, source: RuntimeAbilitySource) -> Result<Self> {
        let snapshot = game.information_snapshot(source.holder)?;
        let id = game
            .state
            .ir_objects
            .iter()
            .find(|(_, number)| **number == source.holder.id.0)
            .map(|(id, _)| id.as_str())
            .ok_or_else(|| invalid("missing source identity"))?;
        if snapshot.view.object != source.holder {
            return Err(invalid("foreign source snapshot"));
        }
        Ok(Self {
            holder: source.holder,
            zone: game.object(id)?.zone.clone(),
            departed: None,
        })
    }

    pub(super) fn snapshot(
        &self,
        game: &Game,
        source: RuntimeAbilitySource,
    ) -> Result<SourceSnapshot> {
        if source.holder != self.holder {
            return Err(invalid("source information belongs to another ability"));
        }
        if let Some(departed) = &self.departed {
            return Ok(departed.clone());
        }
        let snapshot = game.information_snapshot(self.holder)?;
        let id = game
            .state
            .ir_objects
            .iter()
            .find(|(_, number)| **number == self.holder.id.0)
            .map(|(id, _)| id.as_str())
            .ok_or_else(|| invalid("missing source identity"))?;
        if game.object(id)?.zone != self.zone {
            return Err(invalid(
                "source left its reference zone without authority LKI",
            ));
        }
        Ok(snapshot)
    }

    pub(super) fn seal(&mut self, snapshot: Option<SourceSnapshot>) {
        if self.departed.is_none()
            && let Some(snapshot) = snapshot
        {
            self.departed = Some(snapshot);
        }
    }
}

impl SourceSnapshot {
    pub(super) const fn original_cost(&self) -> i64 {
        self.original[0]
    }

    pub(super) const fn at(mut self, snapshot: SnapshotId) -> Self {
        self.view.snapshot = snapshot;
        self
    }

    pub(super) const fn view(&self) -> RuntimeObjectView {
        self.view
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Pending and executing abilities share the same numeric information boundary."
)]
impl Game {
    pub(super) fn information_snapshot(&self, reference: ObjectRef) -> Result<SourceSnapshot> {
        let id = self
            .state
            .ir_objects
            .iter()
            .find(|(_, number)| **number == reference.id.0)
            .map(|(id, _)| id.as_str())
            .ok_or_else(|| invalid("unregistered source identity"))?;
        let current = self.object(id)?;
        if current.generation != reference.generation.0 {
            return Err(invalid("stale Current information read"));
        }
        let printed = self.face(id)?;
        let cost = super::cost::printed_cost(&self.internal_cost_print(id)?["cost"])?;
        // A non-follower has no attack/life; only a valid view can yield rules-absent zero.
        let (original_attack, original_life, attack, life) =
            if self.object_type(id)?.contains("フォロワー") {
                let power = super::cost::printed_cost(&printed["power"])?;
                let life = super::cost::printed_cost(&printed["hp"])?;
                match current.zone.as_str() {
                    "field" => (
                        power,
                        life,
                        self.internal_numeric_stat(id, Stat::Attack)?,
                        self.internal_numeric_stat(id, Stat::Life)?,
                    ),
                    "hand" | "ex" | "cemetery"
                        if matches!(
                            self.object_type(id)?,
                            "フォロワー" | "フォロワー・アドバンス"
                        ) =>
                    {
                        (power, life, power, life)
                    }
                    _ => return Err(unsupported("off-field follower source information")),
                }
            } else {
                (0, 0, 0, 0)
            };
        let original = [cost, original_attack, original_life];
        let effective = [self.internal_effective_cost(id)?, attack, life];
        Ok(SourceSnapshot {
            view: RuntimeObjectView {
                object: reference,
                snapshot: SnapshotId(self.state.ir_epoch),
            },
            original,
            effective,
            names: vec![CardName(self.card_name(id)?)],
            counters: current
                .state
                .get("counters")
                .and_then(Value::as_object)
                .map(|counters| {
                    counters
                        .keys()
                        .map(|kind| {
                            self.card_counter(id, &CounterKind(kind.clone()))
                                .map(|count| (kind.clone(), count))
                        })
                        .collect::<Result<_>>()
                })
                .transpose()?
                .unwrap_or_default(),
        })
    }
}
