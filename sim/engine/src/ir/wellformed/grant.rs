use super::{Checked, Checker};
use crate::ir::player_rules::controller_next_turn_start;
use crate::ir::{
    Duration, DurationEnd, GrantedAbility, Interval, KeywordId, ObjectOperand, ObjectView,
    Selector, TextProvider, TimeAnchor,
};
impl Checker {
    pub(super) fn grant(
        &mut self,
        subjects: &Selector,
        abilities: &[GrantedAbility],
        duration: &Duration,
        provider: &TextProvider,
    ) -> Checked {
        let valid_duration = match duration.intervals.as_slice() {
            [] => {
                matches!(
                    duration.end,
                    DurationEnd::NoTemporalExpiry {} | DurationEnd::CurrentTurnEnd {}
                ) || controller_next_turn_start(&duration.end)
            }
            [Interval::DuringNextTurn { player }] => {
                self.player(player)?;
                matches!(&duration.end, DurationEnd::NextTurnEnd { player: end } if player == end)
            }
            _ => false,
        };
        if abilities.is_empty()
            || duration.anchor != (TimeAnchor::Installation {})
            || duration.usage.is_some()
            || !valid_duration
            || !self.ability_ids.contains(&provider.ability)
            || provider.face.0.is_empty()
            || !matches!(&provider.provider, ObjectView::Current { object } if object.as_ref() == &ObjectOperand::SourceHolder {})
        {
            return self.reject();
        }
        for ability in abilities {
            match ability {
                GrantedAbility::Definition { ability } => self.ability(ability)?,
                GrantedAbility::Keyword {
                    keyword:
                        KeywordId::Ward
                        | KeywordId::Aura
                        | KeywordId::Storm
                        | KeywordId::Rush
                        | KeywordId::Bane
                        | KeywordId::Drain
                        | KeywordId::Intimidate
                        | KeywordId::Assail,
                } => {}
                GrantedAbility::Keyword { .. } => return self.reject(),
            }
        }
        self.view(&provider.provider)?;
        self.selector(subjects)
    }
}
