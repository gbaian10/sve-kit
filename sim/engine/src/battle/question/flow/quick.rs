use super::{AnswerError, BattleReferences, Catalog, Context, ProjectionError, Result};
use crate::battle::{
    question::{common, decision},
    selection, sources,
};
use crate::game::player::{self, QuickChoice, SelectionMember};
use crate::ir::QuickWindow;
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{Answer, Nat, OptionId, SourceRef, Subject, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    coordinates: BTreeMap<OptionId, u64>,
}

pub(super) fn project(
    window: QuickWindow,
    options: &[QuickChoice],
    context: Context<'_>,
    catalog: &Catalog,
    references: &BattleReferences,
) -> Result<(wire::QuestionShape, Binding)> {
    if context.purpose != player::QuestionPurpose::QuickWindow
        || context.observation.viewer != common::seat(context.actor)
    {
        return Err(ProjectionError::Unavailable);
    }
    let mut coordinates = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut projected = Vec::new();
    for (index, option) in options.iter().enumerate() {
        if !seen.insert(option.handle) {
            return Err(ProjectionError::Unavailable);
        }
        let facts = decision::details(
            &SelectionMember {
                handle: option.handle,
                player: context.actor,
                card: Some(option.card.clone()),
                name: None,
                object: option.object.clone(),
                rules_names: None,
                original_cost: None,
            },
            context,
        )?;
        let Subject::Card {
            card,
            object: Some(holder),
        } = facts.subject.ok_or(ProjectionError::Unavailable)?
        else {
            return Err(ProjectionError::Unavailable);
        };
        let origin = facts.origin.ok_or(ProjectionError::Unavailable)?;
        let position = option
            .position
            .checked_sub(1)
            .ok_or(ProjectionError::Unavailable)?;
        if facts.controller != Some(common::seat(context.actor))
            || origin.player != common::seat(context.actor)
            || origin.zone != selection::zone(option.zone)
            || origin.position.map(Nat::get) != Some(position)
        {
            return Err(ProjectionError::Unavailable);
        }
        let (action, source) = if option.line.is_some() {
            let source = option.source.as_ref().ok_or(ProjectionError::Unavailable)?;
            if source.line != option.line {
                return Err(ProjectionError::Unavailable);
            }
            if source.holder != option.object {
                return Err(ProjectionError::Unavailable);
            }
            (
                wire::QuickAction::Ability,
                sources::source_ref(catalog, references, source, Some(holder))?,
            )
        } else {
            // A card play names the authorized effective card, not its Spell body text.
            (
                wire::QuickAction::Play,
                SourceRef::Card {
                    printing_id: card.printing_id,
                    face: Some(card.face),
                    line: None,
                    holder: Some(holder),
                },
            )
        };
        let id = common::option(context.id, index);
        coordinates.insert(id.clone(), option.handle);
        projected.push(wire::QuickOption {
            id,
            action,
            source,
            origin: Some(origin),
        });
    }
    Ok((
        wire::QuestionShape::Quick {
            window: match window {
                QuickWindow::Attack => wire::QuickWindow::Attack,
                QuickWindow::EndPhase => wire::QuickWindow::EndPhase,
            },
            options: projected,
        },
        Binding { coordinates },
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Quick { option } = answer else {
            return Err(AnswerError);
        };
        Ok(player::Answer::Quick(
            option
                .as_ref()
                .map(|id| self.coordinates.get(id).copied().ok_or(AnswerError))
                .transpose()?,
        ))
    }
}
