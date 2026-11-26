//! Absence is a rules result; an invalid identity or missing source proof remains an error.
use super::{Execution, Game, Result, invalid, unsupported};
use crate::ir::{CardKind, NumberRead, ObjectOperand, ObjectView, Selector, stat_presence};

impl Execution {
    pub(super) fn stat_information_present(
        &self,
        game: &Game,
        information: &NumberRead,
    ) -> Result<bool> {
        if !stat_presence::supported(information) {
            return Err(unsupported("stat presence view"));
        }
        let NumberRead::Stats { object, .. } = information else {
            return Err(unsupported("stat presence information"));
        };
        if let ObjectView::Current { object } = object {
            if let ObjectOperand::Selected { selection } = object.as_ref() {
                if !self
                    .targets
                    .get(&selection.key())
                    .is_some_and(super::card::selection::TargetWait::public_board_single)
                {
                    return Err(unsupported(
                        "stat presence of a private or multiple selection",
                    ));
                }
                if self
                    .objects(
                        game,
                        &Selector::Bound {
                            reference: *selection,
                        },
                    )?
                    .is_empty()
                {
                    return Ok(false);
                }
            }
            self.number_information(game, information)?;
            let reference = self.object(game, object)?;
            let id = self.object_id(reference)?;
            return Ok(game.object_type(id)?.contains("フォロワー"));
        }
        // Resolve the number first so unknown provenance never turns into rules absence.
        self.number_information(game, information)?;
        let snapshot = match object {
            ObjectView::SourceInformation {} => self.source_snapshot(game)?,
            ObjectView::EventSnapshot { .. } | ObjectView::LastKnown { .. } => {
                self.event_number_information(game, object)?
            }
            ObjectView::Current { .. }
            | ObjectView::Selected { .. }
            | ObjectView::Snapshot { .. } => return Err(invalid("stat presence shape changed")),
        };
        Ok(snapshot.subject.kind == Some(CardKind::Follower))
    }
}
