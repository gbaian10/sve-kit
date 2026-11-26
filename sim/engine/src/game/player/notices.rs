//! Viewer disclosures frozen at the instruction, independently of the rules journal.

use super::{
    Viewer,
    observation::{CardFace, EffectSource, viewer},
};
use crate::Result;
use crate::battle::{BattleReferences, ProjectionError, source_ref};
use crate::game::Game;
use alloc::sync::Arc;
use core::result;
use sve_battle_contract::{
    Nat, NoticeId, Seat, SourceRef,
    presentation::{Notice, NoticeKind, ShownCard},
};

#[derive(Clone)]
struct DisclosedCard {
    face: CardFace,
    owner: Seat,
}

#[derive(Clone)]
struct Disclosure {
    kind: NoticeKind,
    source: Option<EffectSource>,
    cards: Vec<DisclosedCard>,
}

#[derive(Clone, Default)]
struct Queue {
    pending: Vec<Arc<Disclosure>>,
    delivered: u64,
}

#[derive(Clone, Default)]
pub(in crate::game) struct Notices {
    p1: Queue,
    p2: Queue,
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}

impl Notices {
    pub(in crate::game) fn truncate_pending_to(&mut self, target: &Self) {
        for (current, saved) in [(&mut self.p1, &target.p1), (&mut self.p2, &target.p2)] {
            // Delivery may have removed a prefix since this authority snapshot was taken.
            let delivered = current.delivered.saturating_sub(saved.delivered);
            let delivered = usize::try_from(delivered).unwrap_or(usize::MAX);
            current
                .pending
                .truncate(saved.pending.len().saturating_sub(delivered));
        }
    }

    const fn queue(&mut self, viewer: Viewer) -> &mut Queue {
        match viewer {
            Viewer::P1 => &mut self.p1,
            Viewer::P2 => &mut self.p2,
        }
    }
}

impl Game {
    pub(in crate::game) fn record_battle_notice(
        &mut self,
        audience: &[Viewer],
        kind: NoticeKind,
        ids: &[String],
        source: impl Fn(&Self, Viewer) -> Option<EffectSource>,
    ) -> Result<()> {
        let ordered = kind == NoticeKind::Look;
        if !ordered && ids.is_empty() {
            return Ok(());
        }
        let mut cards = ids
            .iter()
            .map(|id| {
                let owner = viewer(&self.object(id)?.owner)?;
                Ok(DisclosedCard {
                    face: self.physical_card_face(id)?,
                    owner: seat(owner),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !ordered {
            // Hidden zone ordering stays private even when the faces are revealed.
            cards.sort_by(|left, right| {
                (&left.face.card, left.face.face, left.owner).cmp(&(
                    &right.face.card,
                    right.face.face,
                    right.owner,
                ))
            });
        }
        for &viewer in audience {
            let source = source(self, viewer).map(|mut source| {
                source.holder = None;
                source
            });
            self.player_notices
                .queue(viewer)
                .pending
                .push(Arc::new(Disclosure {
                    kind,
                    source,
                    cards: cards.clone(),
                }));
        }
        Ok(())
    }

    /// Transfer all undelivered short-lived disclosures to this seat's local channel.
    ///
    /// # Errors
    /// Refuses an inconsistent reference background or a disclosure that cannot be represented.
    pub fn take_battle_notices(
        &mut self,
        viewer: Viewer,
        references: &BattleReferences,
    ) -> result::Result<Vec<Notice>, ProjectionError> {
        let queue = self.player_notices.queue(viewer);
        if queue.pending.is_empty() {
            return Ok(vec![]);
        }
        references.validate_catalog(&self.catalog)?;
        let mut delivered = queue.delivered;
        let notices = queue
            .pending
            .iter()
            .map(|notice| {
                delivered = delivered.checked_add(1).ok_or(ProjectionError::Range)?;
                let id = NoticeId::new(format!("n{}", Nat::try_from(delivered)?.get()));
                let source = match notice.source.as_ref() {
                    Some(origin) => match source_ref(&self.catalog, references, origin, None)? {
                        SourceRef::Text { text } => Some(text),
                        SourceRef::Card { .. } => None,
                    },
                    None => None,
                };
                let cards = notice
                    .cards
                    .iter()
                    .enumerate()
                    .map(|(index, card)| {
                        Ok(ShownCard {
                            card: references.resolve(&self.catalog, &card.face)?,
                            owner: card.owner,
                            position: (notice.kind == NoticeKind::Look)
                                .then(|| Nat::try_from(index))
                                .transpose()?,
                        })
                    })
                    .collect::<result::Result<_, ProjectionError>>()?;
                Ok(Notice {
                    id,
                    kind: notice.kind,
                    source,
                    cards,
                })
            })
            .collect::<result::Result<Vec<_>, ProjectionError>>()?;
        queue.delivered = delivered;
        queue.pending.clear();
        Ok(notices)
    }
}
