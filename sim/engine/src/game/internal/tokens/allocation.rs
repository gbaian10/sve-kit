//! Resolves token templates and zone capacity before creating any objects.

use super::super::{Execution, Game, InputDomain, Result, Wait, invalid, seat_name, unsupported};
use super::{TokenBatches, TokenPlan, check_template};
use crate::game::player::{TokenCapacityQuestion, Viewer, observation::ObservedZone};
use crate::ir::{
    Action, DecisionId, Instruction, Nat, Node, PlayerId, Posture, TokenDestination, TokenSpec,
    TokenTemplate, ZoneKind,
};
use core::iter::repeat_n;

pub(super) fn specification(action: &Action) -> Option<&[TokenSpec]> {
    let Action::Token {
        tokens,
        destination: TokenDestination::Split { player, actor, .. },
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects,
        ..
    } = action
    else {
        return None;
    };
    (!tokens.is_empty()
        && entry_effects.is_empty()
        && super::super::alternatives::participant(player)
        && super::super::alternatives::participant(actor)
        && tokens.iter().all(|spec| {
            matches!(&spec.template, TokenTemplate::Fixed { .. })
                && super::CountPlan::compile(&spec.count).is_ok()
        }))
    .then_some(tokens)
}

pub(super) fn check_catalog(game: &Game, action: &Action) -> Result<()> {
    if let Action::Token {
        tokens,
        destination: TokenDestination::Split { zones, .. },
        ..
    } = action
    {
        for spec in tokens {
            let TokenTemplate::Fixed { template } = &spec.template else {
                return Err(unsupported("allocated token template"));
            };
            for zone in zones {
                check_template(game, &template.0, *zone)?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
struct Placement {
    zone: ZoneKind,
    print: String,
    name: String,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct AllocationWait {
    instruction: Instruction,
    player: PlayerId,
    options: Vec<Vec<Placement>>,
}

impl AllocationWait {
    fn names(&self) -> Vec<Vec<String>> {
        self.options
            .iter()
            .map(|option| option.iter().map(|member| member.name.clone()).collect())
            .collect()
    }

    pub(in crate::game::internal) fn question(
        &self,
        game: &Game,
        choices: Vec<Vec<String>>,
    ) -> Result<TokenCapacityQuestion> {
        if choices != self.names() {
            return Err(invalid("token allocation projection mismatch"));
        }
        let faces = self
            .options
            .iter()
            .map(|option| {
                option
                    .iter()
                    .map(|member| game.internal_creation_face(&member.print))
                    .collect()
            })
            .collect::<Result<_>>()?;
        let placements = self
            .options
            .iter()
            .map(|option| {
                option
                    .iter()
                    .map(|member| zone(member.zone).map(|(zone, _)| zone))
                    .collect()
            })
            .collect::<Result<_>>()?;
        let available = choices.iter().map(Vec::len).max().unwrap_or_default();
        let destination = match self.player.seat {
            1 => Viewer::P1,
            2 => Viewer::P2,
            _ => return Err(invalid("invalid token destination player")),
        };
        Ok(TokenCapacityQuestion {
            destination,
            zone: ObservedZone::Field,
            available: Nat(u64::try_from(available).map_err(invalid)?),
            choices,
            faces: Some(faces),
            placements: Some(placements),
        })
    }

    pub(in crate::game::internal) fn validate(&self, handle: u64) -> Result<usize> {
        let index = usize::try_from(handle).map_err(invalid)?;
        self.options
            .get(index)
            .ok_or_else(|| invalid("invalid token allocation"))?;
        Ok(index)
    }
}

struct Lane {
    placement: Placement,
    template: usize,
    zone: usize,
    maximum: usize,
}

// There are at most ten physical spaces across both destinations.
const CAPACITY: usize = 10;

#[expect(
    clippy::indexing_slicing,
    reason = "Lanes contain the indices used to build the capacity and supply vectors."
)]
fn options(
    lanes: &[Lane],
    mut supply: Vec<usize>,
    mut space: Vec<usize>,
    total: usize,
    up_to: bool,
) -> Result<Vec<Vec<Placement>>> {
    fn distribute(
        lanes: &[Lane],
        supply: &mut [usize],
        space: &mut [usize],
        total: usize,
        current: &mut Vec<Placement>,
        found: &mut Vec<Vec<Placement>>,
    ) -> Result<()> {
        let Some((lane, rest)) = lanes.split_first().filter(|_| total > 0) else {
            if found.len() == 20_000 {
                return Err(unsupported("too many token allocations"));
            }
            found.push(current.clone());
            return Ok(());
        };
        let available_tokens = supply[lane.template];
        let available_space = space[lane.zone];
        let maximum = total
            .min(lane.maximum)
            .min(available_tokens)
            .min(available_space);
        let start = current.len();
        for count in (0..=maximum).rev() {
            supply[lane.template] = available_tokens.saturating_sub(count);
            space[lane.zone] = available_space.saturating_sub(count);
            current.extend(repeat_n(lane.placement.clone(), count));
            distribute(
                rest,
                supply,
                space,
                total.saturating_sub(count),
                current,
                found,
            )?;
            current.truncate(start);
        }
        supply[lane.template] = available_tokens;
        space[lane.zone] = available_space;
        Ok(())
    }

    let mut found = Vec::new();
    distribute(
        lanes,
        &mut supply,
        &mut space,
        total,
        &mut Vec::new(),
        &mut found,
    )?;
    if !up_to {
        let largest = found.iter().map(Vec::len).max().unwrap_or_default();
        found.retain(|option| option.len() == largest);
    }
    // Repeated template names describe the same physical selection.
    found.sort();
    found.dedup();
    Ok(found)
}

fn zone(kind: ZoneKind) -> Result<(ObservedZone, &'static str)> {
    match kind {
        ZoneKind::Field => Ok((ObservedZone::Field, "field")),
        ZoneKind::Ex => Ok((ObservedZone::Ex, "ex")),
        _ => Err(invalid("invalid token allocation zone")),
    }
}

impl Game {
    pub(super) fn allocate_split_tokens(
        &mut self,
        execution: &mut Execution,
        plan: &TokenPlan,
    ) -> Result<Option<TokenBatches>> {
        let Action::Token {
            tokens,
            destination:
                TokenDestination::Split {
                    player,
                    actor,
                    zones,
                    total,
                    up_to,
                    max_per_zone,
                },
            ..
        } = &plan.instruction.action
        else {
            return Err(invalid("token allocation action"));
        };
        let player = execution.player(self, player)?;
        let actor = execution.player(self, actor)?;
        let seat = seat_name(player)?;
        let cap = |n: u64| usize::try_from(n).unwrap_or(CAPACITY).min(CAPACITY);
        let supply = plan
            .counts
            .iter()
            .map(|count| count.evaluate(execution, self).map(cap))
            .collect::<Result<Vec<_>>>()?;
        let limit = max_per_zone.map_or(CAPACITY, |n| cap(n.0));
        let mut space = Vec::new();
        let mut lanes = Vec::new();
        for kind in zones {
            let (_, name) = zone(*kind)?;
            let capacity = self.token_capacity(seat, name)?.min(limit);
            if capacity == 0 {
                continue;
            }
            let index = space.len();
            space.push(capacity);
            for (template, (spec, count)) in tokens.iter().zip(&supply).enumerate() {
                let TokenTemplate::Fixed { template: id } = &spec.template else {
                    return Err(invalid("dynamic template in allocation"));
                };
                let print = self.catalog.token_print(&id.0)?;
                let allowed = self
                    .unique_crest_creations(&vec![print.clone(); *count], name, seat)?
                    .len();
                if allowed > 0 {
                    lanes.push(Lane {
                        placement: Placement {
                            zone: *kind,
                            name: self.token_name(&print)?,
                            print,
                        },
                        template,
                        zone: index,
                        maximum: allowed,
                    });
                }
            }
        }
        let total = total.map_or(CAPACITY, |n| cap(n.0));
        let options = options(&lanes, supply, space, total, up_to.0)?;
        let wait = AllocationWait {
            instruction: plan.instruction.clone(),
            player,
            options,
        };
        if let [only] = wait.options.as_slice() {
            return self
                .create_placements(only, player, &wait.instruction)
                .map(Some);
        }
        self.internal_wait(
            execution,
            DecisionId(plan.instruction.id.0),
            actor,
            InputDomain::TokenCapacity {
                choices: wait.names(),
            },
            Wait::TokenAllocation(Box::new(wait)),
        )?;
        Ok(None)
    }

    fn create_placements(
        &mut self,
        placements: &[Placement],
        player: PlayerId,
        instruction: &Instruction,
    ) -> Result<TokenBatches> {
        let Action::Token {
            destination: TokenDestination::Split { zones, .. },
            ..
        } = &instruction.action
        else {
            return Err(invalid("allocation destination"));
        };
        let seat = seat_name(player)?;
        let mut batches = Vec::new();
        for kind in zones {
            let prints = placements
                .iter()
                .filter(|placement| placement.zone == *kind)
                .map(|placement| placement.print.clone())
                .collect::<Vec<_>>();
            if prints.is_empty() {
                continue;
            }
            if prints.len() > self.token_capacity(seat, zone(*kind)?.1)? {
                return Err(invalid("token allocation no longer fits"));
            }
            batches.push((*kind, self.internal_create_prints(&prints, seat)?));
        }
        Ok(batches)
    }

    pub(in crate::game::internal) fn internal_token_allocation_answer(
        &mut self,
        execution: &mut Execution,
        wait: &AllocationWait,
        index: usize,
    ) -> Result<()> {
        let placements = wait
            .options
            .get(index)
            .ok_or_else(|| invalid("invalid token allocation"))?;
        let instruction = &wait.instruction;
        let batches = self.create_placements(placements, wait.player, instruction)?;
        self.commit_token_batches(
            execution,
            instruction,
            batches,
            Node::Instruction {
                instruction: Box::new(instruction.clone()),
            },
        )
    }
}
