//! Activation eligibility is shared without allocating authority instances.

use super::super::grant::EffectiveAbility;
use super::super::unsupported;
use super::{Context, Game, player_id, seat_name};
use super::{activation_type, printed_origin, quick_activation_type, resource_entry};
use crate::ir::quick::is_printed_quick;
use crate::ir::{AbilityInstanceId, AbilityKind, Generation, PlayerId, TurnId, ZoneKind};
use crate::{Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Activation eligibility belongs to the existing game authority."
)]
impl Game {
    pub(in crate::game::internal) fn check_internal_activation(
        &self,
        holder: &str,
        ability: &EffectiveAbility,
        actor: PlayerId,
        instance: Option<AbilityInstanceId>,
    ) -> Result<Context> {
        let object = self.object(holder)?;
        self.internal_require_ability_permission(holder, &ability.node)?;
        if !self.activation_holder_available(holder, ability, actor)? {
            return Err(unsupported("activation holder, controller or timing"));
        }
        if ability.grant.is_none() {
            let (face, _) = printed_origin(&ability.node.source.origin)?;
            let (card, ordinal) = self.internal_effective_print(holder)?;
            if self.printed_face_ordinal(&card, face)? != ordinal {
                return Err(invalid("printed ability belongs to another face"));
            }
        }
        let context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: player_id(&object.controller)?,
        };
        self.internal_context(&context)?;
        self.check_internal_resources()?;
        if resource_entry::intrinsic(&ability.node) {
            self.check_resource_entry_available(holder, &ability.node)?;
        }
        if let Some(instance) = instance
            && let Some(limit) = ability.node.play.as_ref().and_then(|play| play.limit)
            && !self.card_runtime.quota_available(
                instance,
                TurnId(self.state.schedule.current()?),
                limit,
            )
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "activated ability quota exhausted".into(),
            ));
        }
        Ok(context)
    }
    pub(super) fn activation_holder_available(
        &self,
        holder: &str,
        ability: &EffectiveAbility,
        actor: PlayerId,
    ) -> Result<bool> {
        let object = self.object(holder)?;
        let seat = seat_name(actor)?;
        Ok(!(ability.node.kind != AbilityKind::Activated
            || !matches!(
                (object.zone.as_str(), ability.node.active_zones.as_slice()),
                ("field", [ZoneKind::Field])
                    | ("ex", [ZoneKind::Ex])
                    | ("hand", [ZoneKind::Hand])
                    | ("cemetery", [ZoneKind::Cemetery])
            )
            || object.controller != seat
            || !self.internal_play_timing(actor, is_printed_quick(&ability.node))
            // Installed grants follow initial ability loss; equipment grants are filtered upstream.
            || (object.state["silenced"] == true && ability.grant.is_none())
            || !activation_type(self.object_type(holder)?, &ability.node.active_zones)
            || (is_printed_quick(&ability.node)
                && !quick_activation_type(self.object_type(holder)?, &ability.node.active_zones))
            || !self.prepared.is_empty()))
    }
}
