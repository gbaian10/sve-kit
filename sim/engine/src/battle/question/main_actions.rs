//! Each attack owns its target codes; native intent capabilities never enter the wire.

use super::{AnswerError, Binding as QuestionBinding, Result, common, decision::Context};
use crate::battle::{ProjectionError, sources};
use crate::game::player::{self, MainActionAnswer, MainIntentId, MainIntentKind, QuestionToken};
use alloc::collections::BTreeMap;
use core::result;
use sve_battle_contract::{Answer, ObjectHandle, OptionId, question as wire};

#[derive(Clone)]
struct Intent {
    native: MainIntentId,
    targets: Option<BTreeMap<OptionId, player::AttackTarget>>,
}

#[derive(Clone)]
pub(super) struct Binding {
    intents: BTreeMap<OptionId, Intent>,
}

pub(super) fn project(
    actions: &player::MainActions,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, QuestionBinding)> {
    if actions.intents.is_empty()
        || context.purpose != player::QuestionPurpose::MainAction
        || context.observation.viewer != common::seat(context.actor)
    {
        return Err(ProjectionError::Unavailable);
    }
    let mut seen = Vec::new();
    let mut bindings = BTreeMap::new();
    let mut intents = Vec::new();
    for (index, intent) in actions.intents.iter().enumerate() {
        if seen.contains(&&intent.id) {
            return Err(ProjectionError::Unavailable);
        }
        seen.push(&intent.id);
        let id = common::option(context.id, index);
        let (projected, targets) = intent_kind(&intent.kind, &id, context)?;
        intents.push(projected);
        bindings.insert(
            id,
            Intent {
                native: intent.id.clone(),
                targets,
            },
        );
    }
    Ok((
        wire::QuestionUse::MainAction,
        wire::QuestionShape::ActionWindow { intents },
        QuestionBinding::Main(Binding { intents: bindings }),
    ))
}

type Targets = Option<BTreeMap<OptionId, player::AttackTarget>>;

fn intent_kind(
    kind: &MainIntentKind,
    id: &OptionId,
    context: Context<'_>,
) -> Result<(wire::MainIntent, Targets)> {
    let projected = match kind {
        MainIntentKind::EndPhase => wire::MainIntent::EndPhase { id: id.clone() },
        MainIntentKind::Play { card } => {
            let source = object(&card.handle, context)?;
            let cost = context
                .observation
                .costs
                .cards
                .iter()
                .find(|quote| quote.object == source)
                .map(|quote| quote.cost.clone());
            wire::MainIntent::Play {
                id: id.clone(),
                source,
                cost,
            }
        }
        MainIntentKind::Activate { holder, source, .. } => wire::MainIntent::Activate {
            id: id.clone(),
            holder: object(&holder.handle, context)?,
            ability: source_ref(source.as_ref(), context)?,
        },
        MainIntentKind::Evolve { holder, source, .. } => wire::MainIntent::Evolve {
            id: id.clone(),
            holder: object(&holder.handle, context)?,
            ability: source_ref(source.as_ref(), context)?,
            hint: None,
        },
        MainIntentKind::Attack { attacker, targets } => {
            let attacker = object(&attacker.handle, context)?;
            if targets.is_empty() {
                return Err(ProjectionError::Unavailable);
            }
            let mut coordinates = BTreeMap::new();
            let mut options = Vec::new();
            let mut seen = Vec::new();
            for (index, target) in targets.iter().enumerate() {
                if seen.contains(&target) {
                    return Err(ProjectionError::Unavailable);
                }
                seen.push(target);
                let option = OptionId::new(format!("{}:t{index}", id.as_str()));
                options.push(wire::AttackOption {
                    id: option.clone(),
                    target: attack_target(target, context)?,
                });
                coordinates.insert(option, target.clone());
            }
            return Ok((
                wire::MainIntent::Attack {
                    id: id.clone(),
                    attacker,
                    targets: options,
                },
                Some(coordinates),
            ));
        }
    };
    Ok((projected, None))
}

fn object(native: &player::ObjectHandle, context: Context<'_>) -> Result<ObjectHandle> {
    let handle = ObjectHandle::new(native.as_str().into());
    context
        .observation
        .objects
        .contains_key(&handle)
        .then_some(handle)
        .ok_or(ProjectionError::Unavailable)
}

fn source_ref(
    source: Option<&player::observation::EffectSource>,
    context: Context<'_>,
) -> Result<sve_battle_contract::SourceRef> {
    let source = source.ok_or(ProjectionError::Unavailable)?;
    let holder = source
        .holder
        .as_ref()
        .map(|holder| object(holder, context))
        .transpose()?;
    sources::source_ref(context.catalog, context.references, source, holder)
}

fn attack_target(
    target: &player::AttackTarget,
    context: Context<'_>,
) -> Result<wire::AttackTarget> {
    let opponent = context
        .observation
        .players
        .iter()
        .find(|player| player.seat != context.observation.viewer)
        .ok_or(ProjectionError::Unavailable)?;
    Ok(match target {
        player::AttackTarget::Leader(player) if common::seat(*player) == opponent.seat => {
            wire::AttackTarget::Leader {
                player: opponent.seat,
            }
        }
        player::AttackTarget::Card(card) => {
            let object = object(card, context)?;
            if !opponent
                .zones
                .field
                .cards
                .iter()
                .any(|member| member.object == object)
            {
                return Err(ProjectionError::Unavailable);
            }
            wire::AttackTarget::Card { object }
        }
        player::AttackTarget::Leader(_) => return Err(ProjectionError::Unavailable),
    })
}

impl Binding {
    pub(super) fn answer(
        &self,
        token: QuestionToken,
        answer: &Answer,
    ) -> result::Result<player::Command, AnswerError> {
        let Answer::ActionWindow { intent, target } = answer else {
            return Err(AnswerError);
        };
        let bound = self.intents.get(intent).ok_or(AnswerError)?;
        let decoded = match (&bound.targets, target) {
            (None, None) => MainActionAnswer::None,
            (Some(targets), Some(target)) => {
                MainActionAnswer::Attack(targets.get(target).cloned().ok_or(AnswerError)?)
            }
            (None, Some(_)) | (Some(_), None) => return Err(AnswerError),
        };
        Ok(player::Command::MainAction {
            token,
            intent: bound.native.clone(),
            answer: decoded,
        })
    }
}
