use super::{Checked, Checker};
use crate::ir::{
    Duration, DurationEnd, GrantedAbility, KeywordId, ObjectOperand, ObjectView, Selector,
    TextProvider, TimeAnchor,
};
#[expect(
    clippy::multiple_inherent_impl,
    reason = "Grant validation shares the card's definition identities and lexical environment."
)]
impl Checker {
    pub(super) fn grant(
        &mut self,
        subjects: &Selector,
        abilities: &[GrantedAbility],
        duration: &Duration,
        provider: &TextProvider,
    ) -> Checked {
        if abilities.is_empty()
            || abilities.iter().any(|ability| {
                !matches!(
                    ability,
                    GrantedAbility::Keyword {
                        keyword: KeywordId::Ward
                            | KeywordId::Aura
                            | KeywordId::Storm
                            | KeywordId::Rush
                            | KeywordId::Bane
                            | KeywordId::Intimidate
                            | KeywordId::Assail
                    }
                )
            })
            || duration.anchor != (TimeAnchor::Installation {})
            || duration.usage.is_some()
            || !duration.intervals.is_empty()
            || !matches!(
                duration.end,
                DurationEnd::NoTemporalExpiry {} | DurationEnd::CurrentTurnEnd {}
            )
            || !self.ability_ids.contains(&provider.ability)
            || provider.face.0.is_empty()
            || !matches!(&provider.provider, ObjectView::Current { object } if object.as_ref() == &ObjectOperand::SourceHolder {})
        {
            return self.reject();
        }
        self.view(&provider.provider)?;
        self.selector(subjects)
    }
}
