use super::{
    Action, BodyContext, Cardinality, LoweringError, Nat, ObjectOperand, PlayerRef, PlayerRelation,
    PlayerSet, Selector, TextQuantifier, ast, unsupported,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Steal and give lowering share target bindings."
)]
impl BodyContext<'_> {
    pub(super) fn remember_transfer_target(
        &mut self,
        target: &ast::Target,
        bounds: Cardinality,
        dynamic: bool,
        resolution: bool,
    ) {
        if !resolution
            && !dynamic
            && bounds.minimum == Nat(1)
            && bounds.maximum == Some(Nat(1))
            && matches!(target.from.who, Some(ast::Side::Opponent))
            && matches!(
                target.from.zone,
                Some(ast::TargetZone::Single(ast::TargetSingleZone::Field))
            )
            && matches!(target.from.kind, Some(ast::SearchKind::Follower))
        {
            self.transfer_targets.insert(target.bind.clone());
        }
    }

    pub(super) fn steal(&self, value: &ast::Steal, path: &str) -> Result<Action, LoweringError> {
        if !self.transfer_targets.contains(&value.r#ref) {
            return Err(unsupported(path));
        }
        let selection = *self
            .targets
            .get(&value.r#ref)
            .ok_or_else(|| unsupported(path))?;
        Ok(Action::Transfer {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::Selected { selection }],
            },
            destination_controller: PlayerRef::Controller {},
        })
    }

    /// Gives the chosen objects to their recipient; `other_players` excludes the giving player,
    /// which inside a player procedure is that procedure's player.
    pub(super) fn give(&self, value: &ast::Give, path: &str) -> Result<Action, LoweringError> {
        let selection = *self
            .targets
            .get(&value.r#ref)
            .ok_or_else(|| unsupported(path))?;
        let one = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
        let destination_controller = match &value.to {
            ast::GiveRecipient::OtherPlayers => PlayerRef::RequireOne {
                players: Box::new(PlayerSet {
                    relation: PlayerRelation::Opponents,
                    relative_to: self
                        .procedure_player
                        .map_or(PlayerRef::Controller {}, |binding| PlayerRef::Bound {
                            binding,
                        }),
                    cardinality: one,
                    text_quantifier: TextQuantifier::All,
                }),
            },
            ast::GiveRecipient::Player(side) => match self.side_set(side, path)? {
                (PlayerRelation::SelfPlayer, player) => player,
                (PlayerRelation::Opponents, relative_to) => PlayerRef::RequireOne {
                    players: Box::new(PlayerSet {
                        relation: PlayerRelation::Opponents,
                        relative_to,
                        cardinality: one,
                        text_quantifier: TextQuantifier::One,
                    }),
                },
                (PlayerRelation::All | PlayerRelation::Bound, _) => return Err(unsupported(path)),
            },
        };
        // The whole chosen set: an empty choice gives nothing, and each member moves alone.
        Ok(Action::Transfer {
            objects: Selector::Bound {
                reference: selection,
            },
            destination_controller,
        })
    }
}
