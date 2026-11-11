//! Question links retain the identification period of their frozen candidates.

use super::super::observation::ObjectLink;
use super::super::{DistributionMember, Execution, Game, OrderCard, Result, TargetDomain, invalid};
use crate::game::player::{
    CardOption, ObjectHandle, SelectionGroup, SelectionMember, SelectionQuestion, SelectionRole,
    Viewer,
};
use crate::ir::{ObjectReceipt, ObjectReceiptList, ObjectRef, ReceiptLocation, Selector};

#[derive(Clone, Debug, Default)]
pub(in crate::game::internal) struct CandidateLinks {
    sources: Vec<(ObjectRef, Option<ObjectLink>)>,
}

impl CandidateLinks {
    pub(in crate::game::internal) fn capture(
        game: &Game,
        execution: &Execution,
        members: impl IntoIterator<Item = ObjectRef>,
    ) -> Result<Self> {
        Ok(Self {
            sources: members
                .into_iter()
                .map(|member| {
                    Ok((
                        member,
                        Some(ObjectLink::capture(
                            game,
                            execution.object_id(member)?,
                            member.generation.0,
                        )),
                    ))
                })
                .collect::<Result<_>>()?,
        })
    }

    fn look_sources<'execution>(
        selector: &Selector,
        execution: &'execution Execution,
        sources: &mut Vec<&'execution Self>,
    ) -> Result<bool> {
        let only_look = match selector {
            Selector::Filter { input, .. } | Selector::Extremum { input, .. } => {
                Self::look_sources(input, execution, sources)?
            }
            Selector::Union { sets } => {
                let mut only_look = true;
                for set in sets {
                    only_look &= Self::look_sources(set, execution, sources)?;
                }
                only_look
            }
            Selector::Intersection { sets } => {
                let mut includes_look = false;
                for set in sets {
                    includes_look |= Self::look_sources(set, execution, sources)?;
                }
                includes_look
            }
            Selector::Difference { input, except } => {
                let only_look = Self::look_sources(input, execution, sources)?;
                Self::look_sources(except, execution, sources)?;
                only_look
            }
            Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look { receipt },
                ..
            } => {
                let ReceiptLocation::Local { id } = receipt.location() else {
                    return Err(invalid("untracked iterated Look candidate links"));
                };
                sources.push(
                    execution
                        .player_receipt_links
                        .get(id)
                        .ok_or_else(|| invalid("missing frozen Look candidate links"))?,
                );
                true
            }
            // Native collection consumers do not admit Look lists; never infer a new period.
            Selector::CollectedMembers {
                receipts: ObjectReceiptList::Look { .. },
                ..
            } => return Err(invalid("untracked collected Look candidate links")),
            _ => false,
        };
        Ok(only_look)
    }

    pub(in crate::game::internal) fn for_selectors<'selector>(
        game: &Game,
        execution: &Execution,
        members: impl IntoIterator<Item = ObjectRef>,
        selectors: impl IntoIterator<Item = &'selector Selector>,
    ) -> Result<Self> {
        let mut links = Self::capture(game, execution, members)?;
        let mut receipts = Vec::new();
        let mut only_look = true;
        for selector in selectors {
            only_look &= Self::look_sources(selector, execution, &mut receipts)?;
        }
        for (member, link) in &mut links.sources {
            let mut originals = receipts.iter().filter_map(|saved| {
                saved
                    .sources
                    .iter()
                    .find(|(candidate, _)| candidate == member)
                    .map(|(_, captured)| captured)
            });
            let Some(original) = originals.next() else {
                if only_look && !receipts.is_empty() {
                    return Err(invalid("missing frozen Look candidate member"));
                }
                continue;
            };
            // A union must not replace one receipt's period with another receipt's newer one.
            *link = if originals.all(|other| other == original) {
                original.clone()
            } else {
                None
            };
        }
        Ok(links)
    }

    pub(in crate::game::internal) fn observed(
        &self,
        game: &Game,
        actor: Viewer,
        reference: Option<ObjectRef>,
    ) -> Result<Option<ObjectHandle>> {
        let Some(reference) = reference else {
            return Ok(None);
        };
        Ok(self
            .sources
            .iter()
            .find(|(candidate, _)| *candidate == reference)
            .map(|(_, link)| link)
            .ok_or_else(|| invalid("missing frozen candidate link"))?
            .as_ref()
            .and_then(|link| link.observed(game, actor)))
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Question projection reads the authority's frozen candidate sources."
)]
impl Game {
    pub(in crate::game::internal) fn player_members(
        &self,
        actor: Viewer,
        members: &[DistributionMember],
        references: &[Option<ObjectRef>],
        links: &CandidateLinks,
    ) -> Result<Vec<SelectionMember>> {
        if members.len() != references.len() {
            return Err(invalid("inconsistent selection candidates"));
        }
        members
            .iter()
            .zip(references)
            .map(|(member, reference)| {
                Ok(SelectionMember {
                    handle: member.handle,
                    player: match member.player.seat {
                        1 => Viewer::P1,
                        2 => Viewer::P2,
                        _ => return Err(invalid("invalid selection player")),
                    },
                    card: member.card.clone(),
                    name: member.name.clone(),
                    object: links.observed(self, actor, *reference)?,
                    rules_names: None,
                    original_cost: None,
                })
            })
            .collect()
    }

    pub(in crate::game::internal) fn player_selection(
        &self,
        actor: Viewer,
        domain: &TargetDomain,
        references: &[Option<ObjectRef>],
        links: &CandidateLinks,
    ) -> Result<SelectionQuestion> {
        let mut members = self.player_members(actor, &domain.members, references, links)?;
        for member in &mut members {
            member.original_cost = domain
                .original_costs
                .as_ref()
                .and_then(|costs| costs.get(&member.handle))
                .copied();
        }
        // Native domains use the presence of this map specifically for distinct-by-cost.
        let distinct_original_costs = domain.original_costs.is_some();
        Ok(SelectionQuestion {
            members,
            cardinality: domain.cardinality,
            distinct_names: domain.distinct_names,
            original_costs: domain.original_costs.clone(),
            distinct_original_costs,
            groups: vec![SelectionGroup {
                handles: domain.members.iter().map(|member| member.handle).collect(),
                cardinality: domain.cardinality,
            }],
            roles: domain
                .roles
                .iter()
                .map(|role| SelectionRole {
                    position: role.position,
                    role: role.role,
                    cardinality: role.cardinality,
                })
                .collect(),
            minimum_original_cost: None,
            maximum_original_cost: None,
        })
    }

    pub(in crate::game::internal) fn player_cards(
        &self,
        actor: Viewer,
        cards: &[OrderCard],
        references: &[Option<ObjectRef>],
        links: &CandidateLinks,
    ) -> Result<Vec<CardOption>> {
        if cards.len() != references.len() {
            return Err(invalid("inconsistent printed candidates"));
        }
        cards
            .iter()
            .zip(references)
            .map(|(card, reference)| {
                Ok(CardOption {
                    handle: card.handle,
                    card: card.card.clone(),
                    name: card.name.clone(),
                    object: links.observed(self, actor, *reference)?,
                    rules_names: None,
                    original_cost: None,
                })
            })
            .collect()
    }
}
