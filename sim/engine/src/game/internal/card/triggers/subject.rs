//! Event subject fields avoid unrelated title or ability bindings.
use super::{Game, Result, invalid, unsupported};
use crate::game::Frame;
use crate::game::internal::{SubjectHistoryEvent, SubjectHistoryFact};
use crate::ir::events::subject::SubjectInformation;
use crate::ir::{CardKind, CardName, ClassId, TraitId};

pub(super) struct EventSubjectInformation {
    kind: CardKind,
    pub(super) token: bool,
    class: ClassId,
    traits: Vec<TraitId>,
    names: Vec<CardName>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The card play commit boundary owns observer admission."
)]
impl Game {
    // No admitted modifier changes class or traits; this subset avoids unrelated title bindings.
    pub(super) fn event_subject_information(
        &self,
        source: &str,
    ) -> Result<EventSubjectInformation> {
        let names = self.card_names(source)?.into_iter().map(CardName).collect();
        self.public_subject_information(source, names)
    }

    fn public_subject_information(
        &self,
        source: &str,
        names: Vec<CardName>,
    ) -> Result<EventSubjectInformation> {
        let kind = match self.object_type(source)? {
            kind if kind.contains("フォロワー") => CardKind::Follower,
            "スペル" | "スペル・トークン" | "スペル・アドバンス" => {
                CardKind::Spell
            }
            "アミュレット" | "アミュレット・トークン" | "アミュレット・エボルヴ" => {
                CardKind::Amulet
            }
            _ => return Err(unsupported("event subject kind")),
        };
        let token = self.object_type(source)?.contains("トークン");
        let face = self.face(source)?;
        let class = ClassId(
            face["card_class"]
                .as_str()
                .ok_or_else(|| invalid("missing subject class"))?
                .into(),
        );
        let traits = face["traits"]
            .as_array()
            .ok_or_else(|| invalid("missing subject traits"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(|text| TraitId(text.into()))
                    .ok_or_else(|| invalid("invalid subject trait"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(EventSubjectInformation {
            kind,
            token,
            class,
            traits,
            names,
        })
    }
}

impl EventSubjectInformation {
    pub(super) fn borrowed(&self) -> SubjectInformation<'_> {
        SubjectInformation {
            kind: self.kind,
            class: Some(&self.class),
            traits: &self.traits,
            names: &self.names,
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "History freezes the same public event subject at commit."
)]
impl Game {
    pub(in crate::game) fn internal_departure_history(
        &mut self,
        id: &str,
        destination: Option<&str>,
        frame: &Frame,
    ) -> Result<Option<SubjectHistoryFact>> {
        let Some(to) = destination else {
            return Ok(None);
        };
        if self.card_runtime.is_empty() || self.object(id)?.zone != "field" || to == "field" {
            return Ok(None);
        }
        let destroyed = to == "cemetery"
            && frame
                .values
                .get("destroyed_subjects")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|members| members.iter().any(|value| value.as_str() == Some(id)));
        self.internal_subject_history(id, SubjectHistoryEvent::Leave, Some(to), destroyed)
            .map(Some)
    }

    pub(in crate::game) fn internal_commit_subject_history(
        &mut self,
        observations: Vec<SubjectHistoryFact>,
    ) -> Result<()> {
        for fact in observations {
            let current = self
                .state
                .ir_objects
                .iter()
                .find(|(_, number)| **number == fact.object)
                .and_then(|(id, _)| self.state.objects.get(id))
                .ok_or_else(|| invalid("missing committed history subject"))?;
            if current.generation != fact.generation {
                self.state.ir_subject_history.push(fact);
            }
        }
        Ok(())
    }

    pub(in crate::game) fn internal_subject_history(
        &mut self,
        source: &str,
        event: SubjectHistoryEvent,
        destination: Option<&str>,
        destroyed: bool,
    ) -> Result<SubjectHistoryFact> {
        let names = match self.card_names(source) {
            Ok(names) => Some(names),
            Err(crate::EngineFailure::Unsupported(_)) => None,
            Err(error) => return Err(error),
        };
        let information = self.public_subject_information(source, Vec::new())?;
        let object = self.internal_current_reference(source)?;
        Ok(SubjectHistoryFact {
            event,
            destination: destination.map(str::to_owned),
            object: object.id.0,
            generation: object.generation.0,
            controller: super::player_id(&self.object(source)?.controller)?.seat,
            active: super::player_id(self.active())?.seat,
            kind: match information.kind {
                CardKind::Follower => "follower",
                CardKind::Spell => "spell",
                CardKind::Amulet => "amulet",
                CardKind::EvolveFollower => return Err(unsupported("history subject kind")),
            }
            .into(),
            class: information.class.0,
            traits: information.traits.into_iter().map(|id| id.0).collect(),
            names,
            token: information.token,
            destroyed,
        })
    }
}
