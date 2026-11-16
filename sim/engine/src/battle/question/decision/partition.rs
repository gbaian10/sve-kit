use super::candidates::Candidates;
use super::{AnswerError, Context, Result};
use crate::battle::ProjectionError;
use crate::battle::question::common::seat;
use crate::battle::selection::{bounds, zone};
use crate::game::player::observation::ObservedZone;
use crate::game::player::{self, SearchPartitionQuestion};
use alloc::collections::BTreeSet;
use core::result;
use sve_battle_contract::{Answer, Nat, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    candidates: Candidates,
    buckets: Vec<wire::PartitionBucket>,
}

pub(super) fn project(
    domain: &SearchPartitionQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let facts = domain.facts.as_ref().ok_or(ProjectionError::Unavailable)?;
    let source = &facts.candidates;
    let count =
        u64::try_from(domain.selection.cards.len()).map_err(|_range| ProjectionError::Range)?;
    let cardinality = domain.selection.cardinality;
    let maximum = cardinality.maximum.map_or(count, |value| value.0);
    // Admission has no deck placement or answer-selected ordering; future producers must update this seam.
    if context.observation.viewer != seat(context.actor)
        || source.origin.player != context.actor
        || source.origin.zone != ObservedZone::Deck
        || source.faces.len() != domain.selection.cards.len()
        // Existing admission always sends searched cards to the searcher's own zones.
        || facts.destination.player != source.origin.player
        || !matches!(
            facts.destination.zone,
            ObservedZone::Hand | ObservedZone::Cemetery | ObservedZone::Ex | ObservedZone::Field
        )
        || facts.position.is_some()
        || facts.ordered
        || facts.remainder_destination.is_some()
        || cardinality.minimum.0 > maximum
        || maximum > count
    {
        return Err(ProjectionError::Unavailable);
    }
    let candidates = Candidates::printed(&domain.selection.cards, context, |card| {
        super::search::details(card, source, context)
    })?;
    let buckets = vec![
        wire::PartitionBucket {
            bucket: Nat::ZERO,
            destination: Some(wire::Destination {
                player: seat(facts.destination.player),
                zone: zone(facts.destination.zone),
                position: None,
            }),
            bounds: bounds(cardinality)?,
            ordered: facts.ordered,
        },
        wire::PartitionBucket {
            bucket: Nat::from(1_u32),
            destination: None,
            bounds: wire::Bounds {
                min: Nat::try_from(
                    count
                        .checked_sub(maximum)
                        .ok_or(ProjectionError::Unavailable)?,
                )?,
                max: Some(Nat::try_from(
                    count
                        .checked_sub(cardinality.minimum.0)
                        .ok_or(ProjectionError::Unavailable)?,
                )?),
            },
            ordered: false,
        },
    ];
    Ok((
        wire::QuestionUse::SearchPartition,
        wire::QuestionShape::Partition {
            items: candidates.values.clone(),
            buckets: buckets.clone(),
        },
        super::Binding::Partition(Binding {
            candidates,
            buckets,
        }),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Partition { buckets } = answer else {
            return Err(AnswerError);
        };
        if buckets.len() != self.buckets.len() {
            return Err(AnswerError);
        }
        let mut selected = None;
        let mut all = vec![];
        let mut seen = BTreeSet::new();
        for bucket in buckets {
            let expected = self
                .buckets
                .iter()
                .find(|expected| expected.bucket == bucket.bucket)
                .ok_or(AnswerError)?;
            let count = u64::try_from(bucket.items.len()).map_err(|_range| AnswerError)?;
            if !seen.insert(bucket.bucket)
                || count < expected.bounds.min.get()
                || expected.bounds.max.is_some_and(|max| count > max.get())
            {
                return Err(AnswerError);
            }
            all.extend(bucket.items.iter().cloned());
            if bucket.bucket == Nat::ZERO {
                selected = Some(self.candidates.lookup(&bucket.items)?);
            }
        }
        if self.candidates.lookup(&all)?.len() != self.candidates.values.len() {
            return Err(AnswerError);
        }
        Ok(player::Answer::SearchPartition(
            selected.ok_or(AnswerError)?,
        ))
    }
}
