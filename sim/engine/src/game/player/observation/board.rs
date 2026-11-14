//! Public board links retain the authority's participant lifetimes.

use super::{EffectRecipient, ObjectHandle, Viewer};
use crate::game::{Game, Object};
use crate::{Result, invalid};
use alloc::collections::BTreeSet;

/// An attack whose frozen participants still occupy the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackObservation {
    /// Currently identifiable attacking follower.
    pub attacker: ObjectHandle,
    /// Currently identifiable defending follower or the defending leader.
    pub target: EffectRecipient,
}

/// Persistent attachment categories produced by the current authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RelationshipKind {
    /// An evolution form attached to its field follower.
    Evolution,
    /// An equipment token attached to its bearer.
    Equipment,
    /// A `DrivePoint` paid for Ride.
    Ride,
    /// A Meal resource attached for racing.
    Race,
}

/// One current attachment, ordered only by disclosed kind and opaque endpoints.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Relationship {
    /// Rules meaning of this attachment.
    pub kind: RelationshipKind,
    /// Attached card.
    pub from: ObjectHandle,
    /// Field card carrying the attachment.
    pub to: ObjectHandle,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Board observations share the attack authority and current knowledge."
)]
impl Game {
    pub(super) fn observed_relationships(&self, perspective: Viewer) -> Result<Vec<Relationship>> {
        let mut relationships = BTreeSet::new();
        for object in self.state.objects.values() {
            if !self.player_knows_object(perspective, &object.id) {
                continue;
            }
            match object.zone.as_str() {
                "field" => {
                    self.observed_field_relationships(perspective, object, &mut relationships)?
                }
                "equipment" => {
                    if let Some(link) = object
                        .state
                        .get("equipped_to")
                        .filter(|value| !value.is_null())
                    {
                        relationships.extend(
                            self.observed_relationship(
                                perspective,
                                RelationshipKind::Equipment,
                                &object.id,
                                link.as_str()
                                    .ok_or_else(|| invalid("invalid equipment link"))?,
                            )?,
                        );
                    }
                }
                _ => {}
            }
        }
        Ok(relationships.into_iter().collect())
    }

    fn observed_field_relationships(
        &self,
        perspective: Viewer,
        object: &Object,
        relationships: &mut BTreeSet<Relationship>,
    ) -> Result<()> {
        if let Some(link) = object
            .state
            .get("evolved_with")
            .filter(|value| !value.is_null())
        {
            relationships.extend(
                self.observed_relationship(
                    perspective,
                    RelationshipKind::Evolution,
                    link.as_str()
                        .ok_or_else(|| invalid("invalid evolution link"))?,
                    &object.id,
                )?,
            );
        }
        if let Some(links) = object.state.get("links").filter(|value| !value.is_null()) {
            for (kind, members) in links
                .as_object()
                .ok_or_else(|| invalid("invalid resource links"))?
            {
                for member in members
                    .as_array()
                    .ok_or_else(|| invalid("invalid resource links"))?
                {
                    let id = member
                        .as_str()
                        .ok_or_else(|| invalid("invalid resource link"))?;
                    // Hidden endpoints must not disclose even their relationship category.
                    if !self.player_knows_object(perspective, id) {
                        continue;
                    }
                    let kind = match kind.as_str() {
                        "出走" => RelationshipKind::Race,
                        "憑依" => RelationshipKind::Ride,
                        _ => return Err(invalid("unknown resource relationship")),
                    };
                    relationships.extend(self.observed_relationship(
                        perspective,
                        kind,
                        id,
                        &object.id,
                    )?);
                }
            }
        }
        Ok(())
    }

    fn observed_relationship(
        &self,
        perspective: Viewer,
        kind: RelationshipKind,
        from: &str,
        to: &str,
    ) -> Result<Option<Relationship>> {
        if !self.player_knows_object(perspective, from)
            || !self.player_knows_object(perspective, to)
        {
            return Ok(None);
        }
        let zone = match kind {
            RelationshipKind::Evolution => "evolution",
            RelationshipKind::Equipment => "equipment",
            RelationshipKind::Ride => "drive",
            RelationshipKind::Race => "race",
        };
        if self.object(from)?.zone != zone || self.object(to)?.zone != "field" {
            return Ok(None);
        }
        match kind {
            RelationshipKind::Ride | RelationshipKind::Race => {
                if !self.internal_resource_link_current(to, from, zone)? {
                    return Ok(None);
                }
            }
            RelationshipKind::Evolution | RelationshipKind::Equipment => {}
        }
        Ok(Some(Relationship {
            kind,
            from: self.observed_object_handle(perspective, from)?,
            to: self.observed_object_handle(perspective, to)?,
        }))
    }

    pub(super) fn observed_attack(&self, perspective: Viewer) -> Result<Option<AttackObservation>> {
        let flow = &self.state.flow;
        if flow["kind"] != "battle" {
            return Ok(None);
        }
        let attacker = flow["attacker"]
            .as_str()
            .ok_or_else(|| invalid("missing attack participant"))?;
        let target = flow["target"]
            .as_str()
            .ok_or_else(|| invalid("missing attack participant"))?;
        let attacker_generation = flow["attacker_generation"]
            .as_u64()
            .ok_or_else(|| invalid("missing attack generation"))?;
        let source = self.object(attacker)?;
        if source.zone != "field" || source.generation != attacker_generation {
            return Ok(None);
        }
        let target = match target {
            "P1.leader" => EffectRecipient::Leader(Viewer::P1),
            "P2.leader" => EffectRecipient::Leader(Viewer::P2),
            id => {
                let generation = flow["target_generation"]
                    .as_u64()
                    .ok_or_else(|| invalid("missing attack generation"))?;
                let object = self.object(id)?;
                if object.zone != "field" || object.generation != generation {
                    return Ok(None);
                }
                EffectRecipient::Card(self.observed_card(perspective, id)?.handle)
            }
        };
        Ok(Some(AttackObservation {
            attacker: self.observed_card(perspective, attacker)?.handle,
            target,
        }))
    }
}
