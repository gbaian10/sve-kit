use super::tokens::TokenSelection;
use super::{
    Execution, Game, InputDomain, InstructionOutcome, Result, Wait, invalid, seat_name, unsupported,
};
use crate::game::tokens::prints_for_names;
use crate::ir::{
    Action, DecisionId, Instruction, MoveMethod, MoveResult, PlayerId, PlayerRef,
    RuntimeObjectView, Selector, TokenAllocation, TokenTemplateId, TransformOutcome, ZoneKind,
};

enum Templates<'action> {
    Single(&'action TokenTemplateId),
    /// Each template once; the ability's controller decides which templates each EX receives.
    EachOnce {
        templates: &'action [TokenTemplateId],
        decision: DecisionId,
        actor: &'action PlayerRef,
    },
}

impl Templates<'_> {
    fn names(&self) -> Vec<&TokenTemplateId> {
        match self {
            Self::Single(template) => vec![*template],
            Self::EachOnce { templates, .. } => templates.iter().collect(),
        }
    }
}

fn specification(action: &Action) -> Option<(&Selector, Templates<'_>)> {
    let Action::Transform {
        objects,
        into,
        entry_effects,
    } = action
    else {
        return None;
    };
    let templates = match into {
        TokenAllocation::Single { template } => Templates::Single(template),
        TokenAllocation::EachExactlyOnce {
            templates,
            decision,
            actor,
            ..
        } => Templates::EachOnce {
            templates,
            decision: *decision,
            actor,
        },
        TokenAllocation::Unrestricted { .. } => return None,
    };
    (matches!(objects, Selector::Bound { .. }) && entry_effects.is_empty())
        .then_some((objects, templates))
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if matches!(action, Action::Transform { .. }) && specification(action).is_none() {
        return Err(unsupported("bound EX transformation templates"));
    }
    Ok(())
}

pub(super) fn check_catalog(game: &Game, action: &Action) -> Result<()> {
    check_action(action)?;
    if let Some((_, templates)) = specification(action) {
        for template in templates.names() {
            super::tokens::check_template(game, &template.0, ZoneKind::Ex)?;
        }
    }
    Ok(())
}

struct Banishment {
    groups: Vec<(PlayerId, usize)>,
    banished: Vec<RuntimeObjectView>,
    moves: Vec<MoveResult>,
}

/// Template assignment that survives the decider's questions until every EX group is settled.
#[derive(Clone, Debug)]
pub(super) struct TemplatePlan {
    instruction: Instruction,
    decision: DecisionId,
    /// The ability's controller decides; it is not necessarily a destination.
    actor: PlayerId,
    /// Unsettled destinations with their successful banishment counts, next one first.
    groups: Vec<(PlayerId, usize)>,
    remaining: Vec<TokenTemplateId>,
    assigned: Vec<(PlayerId, Vec<String>)>,
    banished: Vec<RuntimeObjectView>,
    moves: Vec<MoveResult>,
}

impl TemplatePlan {
    fn assign(&mut self, game: &Game, prints: Vec<String>) -> Result<()> {
        if self.groups.is_empty() {
            return Err(invalid("template assignment without a destination"));
        }
        let (destination, _) = self.groups.remove(0);
        let mut remaining = Vec::new();
        for template in self.remaining.drain(..) {
            if !prints.contains(&game.catalog.token_print(&template.0)?) {
                remaining.push(template);
            }
        }
        self.remaining = remaining;
        self.assigned.push((destination, prints));
        Ok(())
    }
}

/// The decider picks which remaining templates one destination EX receives.
#[derive(Clone, Debug)]
pub(super) struct TemplateWait {
    plan: TemplatePlan,
    destination: PlayerId,
    count: usize,
    prints: Vec<String>,
    names: Vec<String>,
    choices: Vec<Vec<String>>,
    available: usize,
}

impl TemplateWait {
    pub(super) const fn player_details(&self) -> (PlayerId, ZoneKind, usize) {
        (self.destination, ZoneKind::Ex, self.available)
    }

    pub(super) fn validate(&self, handle: u64) -> Result<Vec<String>> {
        let chosen = usize::try_from(handle)
            .ok()
            .and_then(|index| self.choices.get(index))
            .ok_or_else(|| invalid("invalid transformation template choice"))?;
        prints_for_names(&self.prints, &self.names, chosen)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Transformation reuses authoritative banishment and token creation."
)]
impl Game {
    pub(super) fn internal_transform(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let (objects, templates) = specification(&instruction.action)
            .ok_or_else(|| unsupported("EX transformation shape"))?;
        let Banishment {
            groups,
            banished,
            moves,
        } = self.internal_transform_banish(execution, objects)?;
        match templates {
            Templates::Single(template) => {
                let mut ids = Vec::new();
                for (player, count) in groups {
                    let requests = [(template.0.clone(), u64::try_from(count).map_err(invalid)?)];
                    let super::tokens::TokenAllocation::Created(created) =
                        self.allocate_token_requests(&requests, ZoneKind::Ex, player)?
                    else {
                        return Err(invalid(
                            "single-template transformation requested a capacity choice",
                        ));
                    };
                    ids.extend(created);
                }
                let created = self.commit_token_objects(execution, ZoneKind::Ex, &ids)?;
                execution.receipts.insert(
                    instruction.receipt,
                    InstructionOutcome::Transform(TransformOutcome {
                        banished,
                        created,
                        moves,
                    }),
                );
                Ok(())
            }
            Templates::EachOnce {
                templates,
                decision,
                actor,
            } => {
                // Load-time checks bound the selection; a larger result means a forged program.
                if banished.len() > templates.len() {
                    return Err(invalid("more transformed objects than templates"));
                }
                let actor = execution.player(self, actor)?;
                self.internal_transform_plan(
                    execution,
                    TemplatePlan {
                        instruction: instruction.clone(),
                        decision,
                        actor,
                        groups,
                        remaining: templates.to_vec(),
                        assigned: Vec::new(),
                        banished,
                        moves,
                    },
                )
            }
        }
    }

    /// Banishes the current EX objects and counts the successes of each controller.
    fn internal_transform_banish(
        &mut self,
        execution: &mut Execution,
        objects: &Selector,
    ) -> Result<Banishment> {
        let mut origins = Vec::new();
        let mut players = Vec::new();
        for reference in execution.objects(self, objects)? {
            let Some(object) = self.state.objects.get(execution.object_id(reference)?) else {
                continue;
            };
            if object.generation != reference.generation.0 {
                continue;
            }
            if object.zone != "ex" {
                return Err(unsupported("transformation requires an EX object"));
            }
            let controller = super::player_id(&object.controller)?;
            origins.push((reference, controller));
            if !players.contains(&controller) {
                players.push(controller);
            }
        }
        let members = origins
            .iter()
            .map(|(reference, _)| *reference)
            .collect::<Vec<_>>();
        let (moves, successful) = self.internal_move_members(
            execution,
            &members,
            "banish",
            ZoneKind::Banish,
            MoveMethod::Banish,
            false,
            None,
            None,
        )?;
        let banished = successful
            .into_iter()
            .map(|member| {
                member
                    .before
                    .ok_or_else(|| invalid("transformation missing original generation"))
            })
            .collect::<Result<Vec<_>>>()?;
        let active = super::player_id(self.active())?;
        players.sort_by_key(|player| *player != active);
        let groups = players
            .into_iter()
            .map(|player| {
                let count = banished
                    .iter()
                    .filter(|before| {
                        origins.iter().any(|(reference, controller)| {
                            *reference == before.object && *controller == player
                        })
                    })
                    .count();
                (player, count)
            })
            .filter(|(_, count)| *count > 0)
            .collect();
        Ok(Banishment {
            groups,
            banished,
            moves,
        })
    }

    /// Settles each destination in turn, asking the decider only when a real choice remains.
    fn internal_transform_plan(
        &mut self,
        execution: &mut Execution,
        mut plan: TemplatePlan,
    ) -> Result<()> {
        while let Some(&(destination, count)) = plan.groups.first() {
            let requests = plan
                .remaining
                .iter()
                .map(|template| (template.0.clone(), 1))
                .collect::<Vec<_>>();
            match self.token_selection(&requests, ZoneKind::Ex, destination, Some(count))? {
                TokenSelection::Chosen(prints) => plan.assign(self, prints)?,
                TokenSelection::Choices {
                    prints,
                    names,
                    choices,
                    available,
                } => {
                    let decision = plan.decision;
                    let actor = plan.actor;
                    let domain = InputDomain::TokenCapacity {
                        choices: choices.clone(),
                    };
                    return self.internal_wait(
                        execution,
                        decision,
                        actor,
                        domain,
                        Wait::TransformTemplates(Box::new(TemplateWait {
                            plan,
                            destination,
                            count,
                            prints,
                            names,
                            choices,
                            available,
                        })),
                    );
                }
            }
        }
        let mut ids = Vec::new();
        for (destination, prints) in &plan.assigned {
            ids.extend(self.internal_create_prints(prints, seat_name(*destination)?)?);
        }
        let created = self.commit_token_objects(execution, ZoneKind::Ex, &ids)?;
        execution.receipts.insert(
            plan.instruction.receipt,
            InstructionOutcome::Transform(TransformOutcome {
                banished: plan.banished,
                created,
                moves: plan.moves,
            }),
        );
        Ok(())
    }

    pub(super) fn internal_transform_answer(
        &mut self,
        execution: &mut Execution,
        wait: TemplateWait,
        prints: Vec<String>,
    ) -> Result<()> {
        let space = self.token_capacity(seat_name(wait.destination)?, "ex")?;
        if space.min(wait.count) != wait.available {
            return Err(invalid("EX capacity changed during the template choice"));
        }
        let mut plan = wait.plan;
        plan.assign(self, prints)?;
        self.internal_transform_plan(execution, plan)
    }
}
