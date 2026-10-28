use super::{Execution, Game, InstructionOutcome, Result, Slot, invalid, seat_name, unsupported};
use crate::ir::{
    Action, Expr, Instruction, Nat, Node, PlayerRef, Posture, ReceiptId, TokenDestination,
    TokenOutcome, TokenSpec, ZoneKind,
};

#[derive(Clone, Debug)]
pub(super) struct Creation {
    receipt: ReceiptId,
    ids: Vec<String>,
}

fn specification(action: &Action) -> Option<(&TokenSpec, ZoneKind)> {
    let Action::Token {
        tokens,
        destination: TokenDestination::Single {
            zone: Expr::Literal(zone),
        },
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects,
        ..
    } = action
    else {
        return None;
    };
    let [token] = tokens.as_slice() else {
        return None;
    };
    if zone.player != (PlayerRef::Controller {})
        || !matches!(zone.kind, ZoneKind::Ex | ZoneKind::Field)
        || !entry_effects.is_empty()
    {
        return None;
    }
    match (&token.name, &token.count) {
        (Expr::Literal(name), Expr::Literal(_)) if name.0 == token.template.0 => {
            Some((token, zone.kind))
        }
        _ => None,
    }
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if matches!(action, Action::Token { .. }) && specification(action).is_none() {
        return Err(unsupported(
            "fixed single-name token creation to own EX or field",
        ));
    }
    Ok(())
}

pub(super) fn check_catalog(game: &Game, action: &Action) -> Result<()> {
    check_action(action)?;
    if let Some((token, zone)) = specification(action) {
        let card = game.catalog.token_print(&token.template.0)?;
        if zone == ZoneKind::Field
            && game.catalog.face(&card, 0)?["card_type"] != "フォロワー・トークン"
        {
            return Err(unsupported("field creation requires a follower token"));
        }
        if !game.card_runtime.has_definition(&card) {
            return Err(unsupported("created token requires an admitted whole card"));
        }
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Creation uses the shared physical object and movement boundaries."
)]
impl Game {
    pub(super) fn internal_tokens(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let (token, destination) = specification(&instruction.action)
            .ok_or_else(|| unsupported("token creation shape"))?;
        let Expr::Literal(Nat(count)) = token.count else {
            return Err(invalid("token count"));
        };
        let card = self.catalog.token_print(&token.template.0)?;
        if !self.card_runtime.is_loaded(&card) {
            return Err(unsupported("created token requires an admitted whole card"));
        }
        let seat = seat_name(execution.context.controller)?;
        let zone = if destination == ZoneKind::Field {
            "field"
        } else {
            "ex"
        };
        let ids = if let Some(creation) = execution.creating.take() {
            if creation.receipt != instruction.receipt {
                return Err(invalid("token creation continuation mismatch"));
            }
            creation.ids
        } else {
            let available = u64::try_from(5_i64.saturating_sub(self.zone_count(seat, zone)).max(0))
                .map_err(invalid)?;
            let requested = vec![card; usize::try_from(count.min(available)).map_err(invalid)?];
            let prints = self.unique_crest_creations(&requested, zone, seat)?;
            let mut ids = Vec::new();
            for _ in prints {
                ids.push(self.new_named_object(&token.template.0, seat)?);
            }
            ids
        };
        execution.ids = self.internal_identities()?;
        let before = ids
            .iter()
            .map(|id| execution.reference(self, id))
            .collect::<Result<Vec<_>>>()?;
        if destination == ZoneKind::Field {
            for id in &ids {
                if self.internal_ward_entry_choice(execution, id)? {
                    // Resume this instruction with the same allocated objects after the choice.
                    execution.creating = Some(Creation {
                        receipt: instruction.receipt,
                        ids,
                    });
                    execution.todo.push_front(Node::Instruction {
                        instruction: Box::new(instruction.clone()),
                    });
                    return Ok(());
                }
            }
        }
        self.move_objects(&ids, zone, Some(seat), None, &execution.frame())?;
        if destination == ZoneKind::Field {
            for member in before {
                self.internal_ward_entry_posture(execution, member)?;
            }
        }
        execution.ids = self.internal_identities()?;
        let created = ids
            .iter()
            .map(|id| execution.reference(self, id))
            .collect::<Result<Vec<_>>>()?;
        let Action::Token { output, .. } = &instruction.action else {
            return Err(invalid("token output"));
        };
        execution
            .slots
            .insert(output.key(), Slot::Created(created.clone()));
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Token(TokenOutcome { created }),
        );
        Ok(())
    }
}
