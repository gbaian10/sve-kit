//! Source information belongs to the checked ability execution, never to a caller's snapshot.

use super::{Execution, Game, Result, invalid, unsupported};
use crate::ir::{
    CharacteristicView, NumberRead, ObjectRef, ObjectView, Requirement, Requirements,
    RuntimeAbilitySource, RuntimeObjectView, SnapshotId, Stat,
};

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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Source reads share the execution's private authority proof."
)]
impl Execution {
    /// Binds the authority-derived ability source and its reference zone at body start.
    pub(super) fn bind_source(&mut self, game: &Game, source: RuntimeAbilitySource) -> Result<()> {
        self.ability = Some(source);
        self.source_information = Some(SourceInformation {
            holder: source.holder,
            zone: game.object(self.object_id(source.holder)?)?.zone.clone(),
            departed: None,
        });
        Ok(())
    }

    fn source_snapshot(&self, game: &Game) -> Result<SourceSnapshot> {
        let information = self
            .source_information
            .as_ref()
            .ok_or_else(|| invalid("missing ability source information producer"))?;
        if self
            .ability
            .is_none_or(|source| source.holder != information.holder)
        {
            return Err(invalid("source information belongs to another ability"));
        }
        if let Some(departed) = &information.departed {
            return Ok(departed.clone());
        }
        let id = self.object_id(information.holder)?;
        let current = game.object(id)?;
        if current.generation != information.holder.generation.0 || current.zone != information.zone
        {
            return Err(invalid(
                "source left its reference zone without authority LKI",
            ));
        }
        self.information_snapshot(game, information.holder)
    }

    fn information_snapshot(&self, game: &Game, reference: ObjectRef) -> Result<SourceSnapshot> {
        let id = self.object_id(reference)?;
        let current = game.object(id)?;
        if current.generation != reference.generation.0 {
            return Err(invalid("stale Current information read"));
        }
        let printed = game.face(id)?;
        let cost = super::cost::printed_cost(&printed["cost"])?;
        // A non-follower has no attack/life; only a valid view can yield rules-absent zero.
        let (original_attack, original_life, attack, life) =
            if game.object_type(id)?.contains("フォロワー") {
                if current.zone != "field" {
                    return Err(unsupported("off-field follower source information"));
                }
                (
                    super::cost::printed_cost(&printed["power"])?,
                    super::cost::printed_cost(&printed["hp"])?,
                    game.internal_numeric_stat(id, Stat::Attack)?,
                    game.internal_numeric_stat(id, Stat::Life)?,
                )
            } else {
                (0, 0, 0, 0)
            };
        let original = [cost, original_attack, original_life];
        let effective = [game.internal_effective_cost(id)?, attack, life];
        Ok(SourceSnapshot {
            view: RuntimeObjectView {
                object: reference,
                snapshot: SnapshotId(game.state.ir_epoch),
            },
            original,
            effective,
        })
    }

    pub(super) fn source_before_move(
        &self,
        game: &Game,
        ids: &[String],
    ) -> Result<Option<SourceSnapshot>> {
        let Some(information) = &self.source_information else {
            return Ok(None);
        };
        if information.departed.is_some() || !ids.iter().any(|id| id == &self.context.source) {
            return Ok(None);
        }
        self.source_snapshot(game).map(Some)
    }

    pub(super) const fn seal_source_departure(&mut self, snapshot: Option<SourceSnapshot>) {
        if let Some(information) = &mut self.source_information
            && let Some(snapshot) = snapshot
        {
            information.departed = Some(snapshot);
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
        if let ObjectView::Current { object } = object {
            // Current keeps its pre-existing Effective-only meaning.
            if *view != CharacteristicView::Effective {
                return Err(unsupported("number characteristic view"));
            }
            let reference = self.object(game, object)?;
            let id = self.object_id(reference)?;
            if game.object(id)?.generation != reference.generation.0 {
                return Err(invalid("stale Current information read"));
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
