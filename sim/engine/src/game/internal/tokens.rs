use super::{Execution, Game, InstructionOutcome, Result, Slot, invalid, seat_name, unsupported};
use crate::ir::{
    Action, Expr, Instruction, Nat, PlayerRef, Posture, TokenDestination, TokenOutcome, TokenSpec,
    ZoneKind,
};

fn specification(action: &Action) -> Option<&TokenSpec> {
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
        || zone.kind != ZoneKind::Ex
        || !entry_effects.is_empty()
    {
        return None;
    }
    match (&token.name, &token.count) {
        (Expr::Literal(name), Expr::Literal(_)) if name.0 == token.template.0 => Some(token),
        _ => None,
    }
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if matches!(action, Action::Token { .. }) && specification(action).is_none() {
        return Err(unsupported("fixed single-name token creation to own EX"));
    }
    Ok(())
}

pub(super) fn check_catalog(game: &Game, action: &Action) -> Result<()> {
    check_action(action)?;
    if let Some(token) = specification(action) {
        let card = game.catalog.token_print(&token.template.0)?;
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
        let token = specification(&instruction.action)
            .ok_or_else(|| unsupported("token creation shape"))?;
        let Expr::Literal(Nat(count)) = token.count else {
            return Err(invalid("token count"));
        };
        let card = self.catalog.token_print(&token.template.0)?;
        if !self.card_runtime.is_loaded(&card) {
            return Err(unsupported("created token requires an admitted whole card"));
        }
        let seat = seat_name(execution.context.controller)?;
        let available = u64::try_from(5_i64.saturating_sub(self.zone_count(seat, "ex")).max(0))
            .map_err(invalid)?;
        let mut ids = Vec::new();
        let requested = vec![card; usize::try_from(count.min(available)).map_err(invalid)?];
        let prints = self.unique_crest_creations(&requested, "ex", seat)?;
        for _ in prints {
            ids.push(self.new_named_object(&token.template.0, seat)?);
        }
        self.move_objects(&ids, "ex", Some(seat), None, &execution.frame())?;
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
