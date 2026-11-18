//! Admission owns the supported shapes; the same resolver checks every possible source.

use crate::battle::{BattleReferences, source_ref};
use crate::game::Game;
use crate::game::internal::observation::wording_source;
use crate::game::player::observation::{EffectSource, TextProvenance};
use crate::ir::SourceSpan;
use crate::ir::walk::{self, NodeRef, Traversal, Visitor};
use crate::{EngineFailure, Result, invalid};

struct Sources<'pinned> {
    game: &'pinned Game,
    references: &'pinned BattleReferences,
    number: &'pinned str,
    error: Option<EngineFailure>,
}

impl Sources<'_> {
    fn check(&self, span: &SourceSpan) -> Result<()> {
        let face = self
            .game
            .effect_source_face(self.number, Some(&span.face))?;
        // The admitted span proves its text owner; no live holder or viewer is invented before startup.
        let source = EffectSource {
            provenance: TextProvenance::Printed,
            card: self.number.into(),
            face,
            line: Some(u64::from(span.line)),
            wording: wording_source(span),
            holder: None,
        };
        source_ref(&self.game.catalog, self.references, &source, None)
            .map_err(|_projection| invalid("battle source reference unavailable"))?;
        Ok(())
    }
}

impl Visitor for Sources<'_> {
    fn enter(&mut self, node: NodeRef<'_>, _path: &str) -> Traversal {
        if self.error.is_some() {
            return Traversal::Skip;
        }
        if let NodeRef::Source(span) = node {
            self.error = self.check(span).err();
        }
        Traversal::Descend
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Preflight reads the same admitted definitions as source producers."
)]
impl Game {
    pub(in crate::game) fn validate_battle_sources(
        &self,
        references: &BattleReferences,
    ) -> Result<()> {
        for (number, program) in &self.card_runtime.definitions.cards {
            let mut sources = Sources {
                game: self,
                references,
                number,
                error: None,
            };
            for ability in &program.card().abilities {
                walk::ability(ability, &mut sources);
            }
            if let Some(error) = sources.error {
                return Err(error);
            }
        }
        Ok(())
    }
}
