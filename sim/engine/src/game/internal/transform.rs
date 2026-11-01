use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, CardName, Expr, Instruction, MoveMethod, Nat, Selector, TokenAllocation, TokenSpec,
    TokenTemplateId, TransformOutcome, ZoneKind,
};

fn specification(action: &Action) -> Option<(&Selector, &TokenTemplateId)> {
    let Action::Transform {
        objects,
        into: TokenAllocation::Single { template },
        entry_effects,
    } = action
    else {
        return None;
    };
    (matches!(objects, Selector::Bound { .. }) && entry_effects.is_empty())
        .then_some((objects, template))
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if matches!(action, Action::Transform { .. }) && specification(action).is_none() {
        return Err(unsupported("single-template EX transformation"));
    }
    Ok(())
}

pub(super) fn check_catalog(game: &Game, action: &Action) -> Result<()> {
    check_action(action)?;
    if let Some((_, template)) = specification(action) {
        super::tokens::check_template(game, &template.0, ZoneKind::Ex)?;
    }
    Ok(())
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
        let (objects, template) = specification(&instruction.action)
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
            let specs = [TokenSpec {
                template: template.clone(),
                name: Expr::Literal(CardName(template.0.clone())),
                count: Expr::Literal(Nat(u64::try_from(count).map_err(invalid)?)),
            }];
            ids.extend(
                self.allocate_token_specs(execution, instruction, &specs, ZoneKind::Ex, player)?
                    .ok_or_else(|| {
                        invalid("single-template transformation requested a capacity choice")
                    })?,
            );
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
