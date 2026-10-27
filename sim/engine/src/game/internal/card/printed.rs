//! Effective printed definitions are resolved once at the catalogue boundary.
use super::{Ability, Game, Located, Result, invalid, player_id, printed_origin};
use crate::EngineFailure;
use crate::ir::{
    AssociationId, CardKind, CardName, Characteristics, ClassId, EvolutionState, Generation, Info,
    Int, ObjectRef, RuntimeAbilityReference, Stat, TitleId, TraitId,
};
use serde_json::Value;

fn text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("missing characteristic text"))
}

fn unavailable(
    card: &str,
    face: usize,
    holder: &str,
    generation: u64,
    query: &str,
) -> EngineFailure {
    super::unsupported(&format!(
        "unloaded whole card {card}/{face}, holder {holder} generation {generation}, query {query}: current ability definitions unavailable"
    ))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Printed abilities and immutable event characteristics share the catalogue boundary."
)]
impl Game {
    pub(in crate::game) fn require_internal_abilities(
        &self,
        holder: &str,
        query: &str,
    ) -> Result<()> {
        let object = self.object(holder)?;
        let printed = object.state["evolved_with"].as_str().unwrap_or(holder);
        let (card, face) = self.internal_effective_print(holder)?;
        if self.card_runtime.is_loaded(&card) || self.internal_evolution_resource(printed)? {
            return Ok(());
        }
        Err(unavailable(&card, face, holder, object.generation, query))
    }

    pub(in crate::game::internal) fn internal_effective_print(
        &self,
        holder: &str,
    ) -> Result<(String, usize)> {
        let object = self.object(holder)?;
        let effective = if let Some(form) = object.state["evolved_with"].as_str() {
            let form = self.object(form)?;
            if object.zone != "field"
                || form.zone != "evolution"
                || form.owner != object.owner
                || !self.object_type(&form.id)?.contains("エボルヴ")
            {
                return Err(invalid("invalid evolution information association"));
            }
            form
        } else {
            object
        };
        let face = effective.state["face"]
            .as_u64()
            .ok_or_else(|| invalid("missing printed face"))?;
        let face = usize::try_from(face).map_err(invalid)?;
        self.catalog.face(&effective.card, face)?;
        Ok((effective.card.clone(), face))
    }

    pub(in crate::game::internal) fn internal_printed_abilities(
        &self,
        holder: &str,
    ) -> Result<Vec<Located<Ability>>> {
        let object = self.object(holder)?;
        if self.card_runtime.is_empty() {
            if object.state["silenced"] != true {
                self.internal_effective_print(holder)?;
            }
            return Ok(Vec::new());
        }
        self.require_internal_abilities(holder, "printed ability set")?;
        let (card, face) = self.internal_effective_print(holder)?;
        let Some(program) = self.card_runtime.cards.get(&card) else {
            let printed = self.catalog.face(&card, face)?;
            if printed["text"].as_str() == Some("")
                && printed["sections"].as_array().is_some_and(Vec::is_empty)
            {
                return Ok(Vec::new());
            }
            return Err(unavailable(
                &card,
                face,
                holder,
                object.generation,
                "printed ability set",
            ));
        };
        if object.state["silenced"] == true {
            return Ok(Vec::new());
        }
        Ok(program
            .card()
            .abilities
            .iter()
            .filter(|ability| {
                printed_origin(&ability.node.source.origin).is_ok_and(|(printed, _)| {
                    self.printed_face_ordinal(&card, printed)
                        .is_ok_and(|ordinal| ordinal == face)
                }) && (super::printed_keywords::card_declaration(&ability.node).is_none()
                    || object.zone == "field")
            })
            .cloned()
            .collect())
    }

    pub(in crate::game::internal) fn internal_current_reference(
        &mut self,
        holder: &str,
    ) -> Result<ObjectRef> {
        let id = self
            .internal_identities()?
            .into_iter()
            .find(|(_, physical)| physical == holder)
            .map(|(id, _)| id)
            .ok_or_else(|| invalid("missing current object identity"))?;
        Ok(ObjectRef {
            id,
            generation: Generation(self.object(holder)?.generation),
        })
    }

    fn internal_evolution_link(&mut self, holder: &str) -> Result<Vec<AssociationId>> {
        let object = self.object(holder)?.clone();
        let Some(form) = object.state["evolved_with"].as_str() else {
            return Ok(Vec::new());
        };
        self.internal_effective_print(holder)?;
        let source = self.internal_current_reference(holder)?;
        let form = self.internal_current_reference(form)?;
        let next = AssociationId(
            u64::try_from(self.card_runtime.links.len())
                .map_err(invalid)?
                .checked_add(1)
                .ok_or_else(|| invalid("association identities exhausted"))?,
        );
        let link = *self
            .card_runtime
            .links
            .entry((
                source.id.0,
                source.generation.0,
                form.id.0,
                form.generation.0,
            ))
            .or_insert(next);
        Ok(vec![link])
    }

    pub(in crate::game::internal) fn internal_characteristics(
        &mut self,
        holder: &str,
    ) -> Result<Characteristics> {
        self.event_characteristics(holder, false)
    }

    pub(in crate::game::internal) fn internal_hand_characteristics(
        &mut self,
        holder: &str,
    ) -> Result<Characteristics> {
        if self.object(holder)?.zone != "hand" {
            return Err(invalid("discard characteristics require a hand card"));
        }
        self.event_characteristics(holder, true)
    }

    fn event_characteristics(&mut self, holder: &str, hand: bool) -> Result<Characteristics> {
        self.require_internal_abilities(holder, "event characteristics and LKI abilities")?;
        let object = self.object(holder)?.clone();
        let (card, face) = self.internal_effective_print(holder)?;
        let printed = self.face(holder)?.clone();
        let kind = match self.object_type(holder)? {
            kind if kind.contains("フォロワー") => CardKind::Follower,
            "スペル" => CardKind::Spell,
            "アミュレット" => CardKind::Amulet,
            _ => return Err(EngineFailure::Unsupported("event card kind".into())),
        };
        let traits = printed["traits"]
            .as_array()
            .ok_or_else(|| invalid("unknown printed traits"))?
            .iter()
            .map(|value| text(value).map(|s| TraitId(s.into())))
            .collect::<Result<Vec<_>>>()?;
        let class = Some(ClassId(text(&printed["card_class"])?.into()));
        let title = self
            .catalog
            .rule_bindings
            .title_code(
                printed["title_code"].as_str().unwrap_or_default(),
                printed["title"].as_str().unwrap_or_default(),
            )?
            .map(TitleId);
        let controller = player_id(&object.controller)?;
        let mut abilities = Vec::new();
        for ability in self.internal_printed_abilities(holder)? {
            let source = self.internal_card_source(holder, ability.node.id, controller)?;
            abilities.push(RuntimeAbilityReference {
                holder: source.holder,
                instance: source.instance,
                definition: source.definition,
            });
        }
        let (attack, life) = if kind == CardKind::Follower && hand {
            // Hand stat patches are not admitted; event snapshots retain the printed values.
            (
                Info::Present(Int(text(&printed["power"])?.parse().map_err(invalid)?)),
                Info::Present(Int(text(&printed["hp"])?.parse().map_err(invalid)?)),
            )
        } else if kind == CardKind::Follower {
            (
                Info::Present(Int(self.internal_numeric_stat(holder, Stat::Attack)?)),
                Info::Present(Int(self.internal_numeric_stat(holder, Stat::Life)?)),
            )
        } else {
            (Info::AbsentByRules, Info::AbsentByRules)
        };
        Ok(Characteristics {
            names: vec![CardName(self.card_name(holder)?)],
            kind,
            traits,
            class,
            title,
            cost: Info::Present(Int(self.internal_effective_cost(holder)?)),
            attack,
            life,
            abilities,
            face: self.printed_face_id(&card, face)?,
            evolution: if object.state["super_evolved"] == true {
                EvolutionState::SuperEvolved
            } else if object.state["evolved_with"].is_string() {
                EvolutionState::Evolved
            } else {
                EvolutionState::Base
            },
            links: self.internal_evolution_link(holder)?,
        })
    }
}
