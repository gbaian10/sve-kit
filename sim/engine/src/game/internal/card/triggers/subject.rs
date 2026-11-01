//! Event subject fields avoid unrelated title or ability bindings.
use super::{Game, Result, invalid, unsupported};
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
        let kind = match self.object_type(source)? {
            kind if kind.contains("フォロワー") => CardKind::Follower,
            "スペル" | "スペル・トークン" | "スペル・アドバンス" => {
                CardKind::Spell
            }
            "アミュレット" | "アミュレット・トークン" => CardKind::Amulet,
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
        let names = self.card_names(source)?.into_iter().map(CardName).collect();
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
