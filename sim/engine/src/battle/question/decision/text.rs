use super::{Context, Result};
use crate::battle::question::common::seat;
use crate::battle::{ProjectionError, sources};
use crate::game::player::observation::EffectSource;
use sve_battle_contract::{ObjectHandle, SourceRef};

pub(super) fn project(source: &EffectSource, context: Context<'_>) -> Result<SourceRef> {
    if context.observation.viewer != seat(context.actor) {
        return Err(ProjectionError::Unavailable);
    }
    let holder = source
        .holder
        .as_ref()
        .map(|handle| {
            let key = ObjectHandle::new(handle.as_str().into());
            if !context.observation.objects.contains_key(&key) {
                return Err(ProjectionError::Unavailable);
            }
            Ok(key)
        })
        .transpose()?;
    sources::source_ref(context.catalog, context.references, source, holder)
}
