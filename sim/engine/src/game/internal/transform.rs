use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, DecisionId, Instruction, MoveMethod, Selector, TokenAllocation, TokenTemplateId,
    TransformOutcome, ZoneKind,
};

enum Templates<'action> {
    Single(&'action TokenTemplateId),
    /// Each template once; fewer successful banishments choose which templates to create.
    EachOnce {
        templates: &'action [TokenTemplateId],
        decision: DecisionId,
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
            ..
        } => Templates::EachOnce {
            templates,
            decision: *decision,
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Transformation reuses authoritative banishment and token creation."
)]
impl Game {
    #[expect(
        clippy::too_many_lines,
        reason = "Banishment, per-controller counts and template creation form one transaction."
    )]
    pub(super) fn internal_transform(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let (objects, templates) = specification(&instruction.action)
            .ok_or_else(|| unsupported("EX transformation shape"))?;
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
        if matches!(templates, Templates::EachOnce { .. }) && players.len() > 1 {
            return Err(unsupported(
                "allocated transformation of several controllers",
            ));
        }
        let mut ids = Vec::new();
        for player in players {
            let count = banished
                .iter()
                .filter(|before| {
                    origins.iter().any(|(reference, controller)| {
                        *reference == before.object && *controller == player
                    })
                })
                .count();
            let created = match &templates {
                Templates::Single(template) => {
                    let requests = [(template.0.clone(), u64::try_from(count).map_err(invalid)?)];
                    let super::tokens::TokenAllocation::Created(created) =
                        self.allocate_token_requests(&requests, ZoneKind::Ex, player, None)?
                    else {
                        return Err(invalid(
                            "single-template transformation requested a capacity choice",
                        ));
                    };
                    created
                }
                Templates::EachOnce {
                    templates,
                    decision,
                } => {
                    if count > templates.len() {
                        return Err(invalid("more transformed objects than templates"));
                    }
                    if count == 0 {
                        continue;
                    }
                    let requests = templates
                        .iter()
                        .map(|template| (template.0.clone(), 1))
                        .collect::<Vec<_>>();
                    let resume = super::tokens::TransformResume {
                        decision: *decision,
                        banished: banished.clone(),
                        moves: moves.clone(),
                    };
                    let Some(created) = self.internal_transform_templates(
                        execution,
                        instruction,
                        &requests,
                        player,
                        count,
                        resume,
                    )?
                    else {
                        // The player chooses the templates; the answer commits the outcome.
                        return Ok(());
                    };
                    created
                }
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
}
