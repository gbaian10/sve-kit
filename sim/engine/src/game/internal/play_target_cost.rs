//! Frozen selection names cannot borrow another card, decision or play execution.
use super::{Execution, Game, Result, Slot, invalid, unsupported};
use crate::ir::play_target_cost;
use crate::ir::{
    AbilityKind, BoolRead, CardName, CardProgram, DecisionId, ExecutionId, ExecutionRef, ObjectRef,
    PlayTargetMatch, SourceSpan,
};

#[derive(Clone, Debug)]
pub(super) struct SelectedNames {
    pub(super) execution: ExecutionId,
    pub(super) decision: DecisionId,
    pub(super) member: ObjectRef,
    pub(super) names: Vec<CardName>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "A quote uses the actual declared spell's authority."
)]
impl Execution {
    pub(super) fn play_target_match(&self, game: &Game, read: &BoolRead) -> Result<bool> {
        let BoolRead::CardPlayTarget {
            execution: ExecutionRef::Current {},
            selection,
            predicate,
            ..
        } = read
        else {
            return Err(unsupported("play target cost read"));
        };
        let (card, source) = game.target_cost_source(self)?;
        let parameter = play_target_cost::parameter(card, source, read)
            .ok_or_else(|| invalid("foreign play target quote decision"))?;
        let wait = self
            .targets
            .get(&selection.key())
            .ok_or_else(|| invalid("missing formal target quote answer"))?;
        if wait.decision != *parameter || !wait.public_board_single() {
            return Err(invalid("play target quote decision changed"));
        }
        let names = self
            .selected_target_names
            .get(&selection.key())
            .ok_or_else(|| invalid("missing frozen play target names"))?;
        let Some(Slot::Objects { members, .. }) = self.slots.get(&selection.key()) else {
            return Err(invalid("missing selected play target"));
        };
        if names.execution != self.id
            || names.decision != parameter.id
            || members.as_slice() != [names.member]
        {
            return Err(invalid("play target names belong to another answer"));
        }
        Ok(match predicate {
            PlayTargetMatch::Present {} => true,
            PlayTargetMatch::NameEquals { name } => names.names.contains(name),
            PlayTargetMatch::NameContains { fragment } => {
                names.names.iter().any(|name| name.0.contains(&fragment.0))
            }
        })
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The source is resolved from authority, never from a supplied face or name."
)]
impl Game {
    fn target_cost_source<'game>(
        &'game self,
        execution: &Execution,
    ) -> Result<(&'game CardProgram, &'game SourceSpan)> {
        if !execution.spell {
            return Err(unsupported("play target names outside a spell play"));
        }
        let source = execution
            .ability
            .ok_or_else(|| invalid("missing spell quote authority"))?;
        if source.controller != execution.context.controller
            || execution.ids.get(&source.holder.id) != Some(&execution.context.source)
        {
            return Err(invalid("foreign spell quote authority"));
        }
        let object = self.object(&execution.context.source)?;
        let card = self
            .card_runtime
            .definitions
            .cards
            .get(&object.card)
            .ok_or_else(|| invalid("missing quote card definition"))?
            .card();
        let ability = card
            .abilities
            .iter()
            .find(|ability| ability.node.id == source.definition)
            .filter(|ability| ability.node.kind == AbilityKind::Spell)
            .ok_or_else(|| invalid("quote authority is not this card's spell"))?;
        Ok((card, &ability.source))
    }
    pub(super) fn has_target_cost(
        &self,
        execution: &Execution,
        decision: Option<DecisionId>,
    ) -> Result<bool> {
        let object = self.object(&execution.context.source)?;
        let card = self
            .card_runtime
            .definitions
            .cards
            .get(&object.card)
            .ok_or_else(|| invalid("missing cost card definition"))?
            .card();
        Ok(card
            .card_play_costs
            .iter()
            .flat_map(|face| &face.node.modifiers)
            .filter_map(|modifier| modifier.node.condition.as_ref())
            .any(|condition| play_target_cost::reads(condition, decision)))
    }
}
