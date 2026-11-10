use super::{
    AbilitySource, BodyContext, Located, LoweringError, Node, SourceSpan, ast, unsupported,
};
use crate::ir::advance_entry::{self, AdvanceName, AdvanceUse};
use crate::ir::{CardName, DecisionId, InstructionId, ReceiptId};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The adjacent private choice and movement share the ability's scope allocator."
)]
impl BodyContext<'_> {
    pub(super) fn advance_entry(
        &mut self,
        choice: &ast::Choose,
        consumer: &ast::Step,
        path: &str,
        movement_path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        let ast::ChooseFrom::Advance(query) = &choice.from else {
            return Err(unsupported(path));
        };
        let ast::SelfPlayer::Controller = query.who;
        let ast::EvolveDeckZone::EvolveDeck = query.zone;
        if !matches!(
            (&query.kind, &query.advance_follower, &query.advance_spell),
            (ast::SearchKind::Follower, Some(ast::True), None)
                | (ast::SearchKind::Spell, None, Some(ast::True))
        ) {
            return Err(unsupported(path));
        }
        let advance_name = match (&query.name, &query.name_contains) {
            (Some(name), None) if !name.is_empty() => AdvanceName::Exact(CardName(name.clone())),
            (None, Some(fragment)) if !fragment.is_empty() => {
                AdvanceName::Contains(CardName(fragment.clone()))
            }
            _ => return Err(unsupported(path)),
        };
        if choice.any
            || choice.groups.is_some()
            || choice.distinct_by.is_some()
            || choice.cost_sum_le.is_some()
            || choice.n != Some(1)
            || choice.upto.is_some()
            || choice.may != Some(true)
            || choice.reveal.is_some()
            || choice.order.is_some()
            || choice.by.is_some()
            || self.targets.contains_key(&choice.bind)
            || self.definitions.contains_key(&choice.bind)
            || self.observations.contains_key(&choice.bind)
            || self.searches.contains(&choice.bind)
        {
            return Err(unsupported(path));
        }
        let usage = match (
            &query.kind,
            &query.advance_follower,
            &query.advance_spell,
            consumer,
        ) {
            (ast::SearchKind::Follower, Some(ast::True), None, ast::Step::Move(movement)) => {
                let usage = match movement.to {
                    ast::Destination::Field => AdvanceUse::Field,
                    ast::Destination::Ex => AdvanceUse::Ex,
                    ast::Destination::Deck
                    | ast::Destination::Hand
                    | ast::Destination::Cemetery => return Err(unsupported(movement_path)),
                };
                if !matches!(&movement.r#ref, Some(ast::MoveObjects::Field(ast::Objects::Bound(name))) if *name == choice.bind)
                    || movement.top.is_some()
                    || movement.position.is_some()
                    || movement.order.is_some()
                    || movement.result.is_some()
                    || movement.acted.is_some()
                {
                    return Err(unsupported(movement_path));
                }
                usage
            }
            (ast::SearchKind::Spell, None, Some(ast::True), ast::Step::PlayCard(play))
                if matches!(advance_name, AdvanceName::Exact(_))
                    && play.r#ref == choice.bind
                    && play.cost.is_none() =>
            {
                AdvanceUse::PlaySpell
            }
            _ => return Err(unsupported(movement_path)),
        };
        let mut choice_source = source.clone();
        choice_source.author_path = path.into();
        let mut movement_source = source.clone();
        movement_source.author_path = movement_path.into();
        let id = self.card_allocator.id(movement_path)?;
        let keys = advance_entry::Keys {
            scope: self.card_allocator.scope(path)?,
            candidate: self.card_allocator.scope(path)?,
            decision: DecisionId(self.card_allocator.id(path)?),
            instruction: InstructionId(id),
            receipt: ReceiptId(id),
        };
        Ok(Located {
            source: choice_source.clone(),
            node: Node::Control {
                control: advance_entry::build(
                    &keys,
                    &choice_source,
                    &movement_source,
                    origin,
                    &advance_name,
                    usage,
                ),
            },
        })
    }
}
