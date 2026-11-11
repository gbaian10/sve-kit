//! Live adapters retain each consumer's existing characteristic and source authority.

use super::{
    BooleanProperty, LabelsProperty, NumberProperty, Policy, PreparedCondition, Reader, Resolver,
    SourceNames,
};
use crate::game::internal::{Execution, Game, Result, invalid, unsupported};
use crate::game::list;
use crate::ir::{
    BooleanState, CardName, CharacteristicView, EvolutionState, Expr, Int, Posture, Stat,
};
use serde_json::Value;

pub(in crate::game::internal) struct Operands<'source> {
    pub(in crate::game::internal) game: &'source Game,
    pub(in crate::game::internal) execution: &'source Execution,
}

impl Resolver for Operands<'_> {
    fn number(&self, expression: &Expr<Int>) -> Result<i64> {
        self.execution
            .integer(self.game, expression)
            .map(|value| value.0)
    }

    fn names(&self, source: SourceNames) -> Result<Vec<CardName>> {
        match source {
            SourceNames::Information => self.execution.source_names(self.game),
            SourceNames::Holder => self
                .game
                .card_names(&self.execution.context.source)
                .map(|names| names.into_iter().map(CardName).collect()),
        }
    }
}

struct Candidate<'source> {
    game: &'source Game,
    id: &'source str,
    policy: Policy,
}

pub(in crate::game::internal) fn matches(
    condition: &PreparedCondition,
    game: &Game,
    id: &str,
) -> Result<bool> {
    condition.evaluate(&Candidate {
        game,
        id,
        policy: condition.policy,
    })
}

impl Reader for Candidate<'_> {
    fn boolean(&self, property: &BooleanProperty) -> Result<bool> {
        match property {
            BooleanProperty::Keyword(keyword) => {
                super::super::search::matches_keyword(self.game, self.id, *keyword)
            }
            BooleanProperty::State(BooleanState::TokenStatus) if self.policy == Policy::Search => {
                Ok(self
                    .game
                    .face(self.id)?
                    .get("card_type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kind.contains("トークン")))
            }
            BooleanProperty::State(BooleanState::TokenStatus) => {
                Ok(self.game.object_type(self.id)?.contains("トークン"))
            }
            BooleanProperty::State(BooleanState::Boxed) => Ok(self.game.internal_boxed(self.id)),
            BooleanProperty::State(BooleanState::Racing) => {
                let object = self.game.object(self.id)?;
                Ok(object.zone == "field"
                    && object.state["racing"] == true
                    && !list(&object.state["links"]["出走"]).is_empty())
            }
            BooleanProperty::State(BooleanState::FaceUp) => {
                Ok(self.game.object(self.id)?.state["face_up"] == true)
            }
            BooleanProperty::State(BooleanState::EnteredThisTurn) => {
                let object = self.game.object(self.id)?;
                Ok(object.zone == "field" && object.state["entered_this_turn"] == true)
            }
            BooleanProperty::Posture(expected) => {
                let object = self.game.object(self.id)?;
                Ok(object.zone == "field"
                    && object.state["acted"].as_bool().unwrap_or(false)
                        == (*expected == Posture::Acted))
            }
            BooleanProperty::Evolution(expected) => {
                let object = self.game.object(self.id)?;
                if object.zone != "field" || !self.game.object_type(self.id)?.contains("フォロワー")
                {
                    return Ok(false);
                }
                let actual = if object.state["super_evolved"] == true {
                    EvolutionState::SuperEvolved
                } else if object.state["evolved"] == true {
                    EvolutionState::Evolved
                } else {
                    EvolutionState::Base
                };
                Ok(actual == *expected)
            }
            BooleanProperty::State(_) => Err(unsupported("candidate boolean property")),
        }
    }

    fn number(&self, property: &NumberProperty) -> Result<Option<i64>> {
        let actual = match property {
            NumberProperty::Counter(kind) => {
                i64::try_from(self.game.card_counter(self.id, kind)?.0).map_err(invalid)?
            }
            NumberProperty::Cost(CharacteristicView::Original) => {
                super::super::cost::printed_cost(&self.game.internal_cost_print(self.id)?["cost"])?
            }
            NumberProperty::Cost(CharacteristicView::Effective) => {
                self.game.internal_effective_cost(self.id)?
            }
            NumberProperty::Stat(property) if self.policy != Policy::Search => {
                if self.game.object(self.id)?.zone != "field"
                    || !self.game.object_type(self.id)?.contains("フォロワー")
                {
                    return Ok(None);
                }
                self.game.internal_numeric_stat(self.id, *property)?
            }
            NumberProperty::Stat(Stat::Attack) => {
                if self.game.object_type(self.id)?.contains("フォロワー") {
                    self.game.object(self.id)?.state["power"]
                        .as_i64()
                        .ok_or_else(|| invalid("search candidate power"))?
                } else {
                    0
                }
            }
            _ => return Err(unsupported("candidate numeric characteristic")),
        };
        Ok(Some(actual))
    }

    fn matches_labels(
        &self,
        property: &LabelsProperty,
        matches: &dyn Fn(&str) -> bool,
    ) -> Result<bool> {
        match property {
            LabelsProperty::Names => self.game.card_name_matches(self.id, matches),
            LabelsProperty::Class(CharacteristicView::Original | CharacteristicView::Effective) => {
                Ok(self
                    .game
                    .face(self.id)?
                    .get("card_class")
                    .and_then(Value::as_str)
                    .is_some_and(matches))
            }
            LabelsProperty::Traits(
                CharacteristicView::Original | CharacteristicView::Effective,
            ) if self.policy == Policy::Search => Ok(self
                .game
                .face(self.id)?
                .get("traits")
                .and_then(Value::as_array)
                .is_some_and(|traits| traits.iter().filter_map(Value::as_str).any(matches))),
            LabelsProperty::Traits(CharacteristicView::Effective) => Ok(
                super::super::card::selection::characteristics(self.game, self.id)?
                    .traits
                    .iter()
                    .any(|value| matches(value)),
            ),
            LabelsProperty::Kind(CharacteristicView::Original) => Ok(matches(
                self.game
                    .face(self.id)?
                    .get("card_type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("missing printed kind"))?,
            )),
            LabelsProperty::Kind(CharacteristicView::Effective) => {
                Ok(matches(self.game.object_type(self.id)?))
            }
            LabelsProperty::Kind(_) | LabelsProperty::Class(_) | LabelsProperty::Traits(_) => {
                Err(unsupported("candidate characteristic view"))
            }
        }
    }
}
