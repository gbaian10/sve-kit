use super::super::{Context, Execution, player_id};
use super::{Game, Result};
use crate::ir::{Generation, ObjectId};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Live providers share a read-only execution without installing an effect."
)]
impl Game {
    pub(in crate::game::internal) fn provider_execution(
        &self,
        source: &str,
        caller: Option<&Execution>,
    ) -> Result<Execution> {
        let object = self.object(source)?;
        // Payment selectors must use the caller's identities, without inheriting its private slots.
        let ids = caller.map_or_else(
            || {
                self.state
                    .objects
                    .keys()
                    .cloned()
                    .zip(1_u64..)
                    .map(|(id, number)| (ObjectId(number), id))
                    .collect()
            },
            |execution| execution.ids.clone(),
        );
        // Provider reads cannot recursively capture play counts or inherit private caller slots.
        // Well-formedness excludes AtPlay and AtResolution reads from provider conditions.
        Ok(self.provider_card_execution(
            Context {
                source: source.into(),
                generation: Generation(object.generation),
                controller: player_id(&object.controller)?,
            },
            ids,
        ))
    }
}
