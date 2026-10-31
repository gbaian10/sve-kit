use super::super::{Context, admission, grant, player_id};
use super::{Ability, Game, Located, Result, unsupported};
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, Action, Bool, CharacteristicPatch,
    ContinuousLayer, Control, Duration, DurationEnd, Effect, EffectTarget, Expr, FaceId,
    Generation, GrantedAbility, Instruction, InstructionContext, InstructionId, Node, ObjectId,
    ObjectOperand, ObjectView, PlayerRef, Program, ReceiptId, Requirements, TextProvider,
    TimeAnchor, TimestampBasis, ZoneKind,
};
use alloc::collections::BTreeSet;

pub(super) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability.declarations.iter().all(|effect| {
            matches!(
                effect,
                Effect::Continuous {
                    layer: ContinuousLayer::Ability,
                    patch: CharacteristicPatch::GrantAbility { .. },
                    ..
                }
            )
        })
}

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    if !is_static(ability)
        || ability.active_zones != [ZoneKind::Field]
        || ability.qualification != AbilityQualification::Ordinary
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
        || ability.declarations.is_empty()
    {
        return Err(unsupported("static keyword header"));
    }
    for effect in &ability.declarations {
        let Effect::Continuous {
            source,
            target: EffectTarget::Dynamic { selector },
            condition,
            patch: CharacteristicPatch::GrantAbility { abilities },
            layer: ContinuousLayer::Ability,
            timestamp,
            active_zones,
            duration,
            ..
        } = effect
        else {
            return Err(unsupported("static keyword declaration"));
        };
        if source != &ability.source
            || active_zones != &[ZoneKind::Field]
            || timestamp
                != &(TimestampBasis::ZoneEntry {
                    object: ObjectOperand::SourceHolder {},
                })
            || duration
                != &(Duration {
                    end: DurationEnd::NoTemporalExpiry {},
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![],
                })
        {
            return Err(unsupported("static keyword lifetime"));
        }
        let body = Program {
            scope: ability.scope,
            steps: vec![Located {
                source: located.source.clone(),
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(1),
                        actor: PlayerRef::Controller {},
                        source: ability.source.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        receipt: ReceiptId(1),
                        action: Action::Grant {
                            subjects: selector.clone(),
                            abilities: abilities.clone(),
                            duration: duration.clone(),
                            provider: TextProvider {
                                provider: ObjectView::Current {
                                    object: Box::new(ObjectOperand::SourceHolder {}),
                                },
                                face: FaceId(located.source.face.clone()),
                                ability: ability.id,
                            },
                        },
                    }),
                },
            }],
        };
        let program = Program {
            scope: ability.scope,
            steps: vec![Located {
                source: located.source.clone(),
                node: Node::Control {
                    control: Control::If {
                        condition: condition.clone().unwrap_or(Expr::Literal(Bool(true))),
                        then_body: body,
                        else_body: None,
                    },
                },
            }],
        };
        let requirements = Requirements::derive_program(&Located {
            node: program.clone(),
            source: located.source.clone(),
        });
        if requirements
            .required
            .iter()
            .any(|item| item.capability.code == "read.abilities")
        {
            return Err(unsupported("recursive static keyword dependency"));
        }
        // Reuse executable selector/value admission, without installing a frozen grant.
        admission::check_card(&program, &located.source, &ability.source, None)?;
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Static grants share printed keyword queries."
)]
impl Game {
    pub(super) fn internal_static_keywords(&self, holder: &str) -> Result<BTreeSet<String>> {
        let recipient = self.object(holder)?;
        let mut keywords = BTreeSet::new();
        if recipient.zone != "field" {
            return Ok(keywords);
        }
        for source in self.field_ids() {
            let (card, _) = self.internal_effective_print(&source)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            let static_abilities = self
                .internal_printed_abilities(&source)?
                .into_iter()
                .filter(|located| is_static(&located.node))
                .collect::<Vec<_>>();
            if static_abilities.is_empty() {
                continue;
            }
            let object = self.object(&source)?;
            let ids = self
                .state
                .objects
                .keys()
                .cloned()
                .zip(1_u64..)
                .map(|(id, number)| (ObjectId(number), id))
                .collect();
            let execution = self.card_execution(
                Context {
                    source: source.clone(),
                    generation: Generation(object.generation),
                    controller: player_id(&object.controller)?,
                },
                ids,
            )?;
            let member = execution.reference(self, holder)?;
            for located in static_abilities {
                for effect in &located.node.declarations {
                    let Effect::Continuous {
                        target: EffectTarget::Dynamic { selector },
                        condition,
                        patch: CharacteristicPatch::GrantAbility { abilities },
                        ..
                    } = effect
                    else {
                        return Err(unsupported("static keyword effect"));
                    };
                    if condition
                        .as_ref()
                        .map(|value| execution.boolean(self, value))
                        .transpose()?
                        .is_some_and(|value| !value.0)
                        || !execution.objects(self, selector)?.contains(&member)
                    {
                        continue;
                    }
                    for ability in abilities {
                        let GrantedAbility::Keyword { keyword } = ability else {
                            return Err(unsupported("static granted ability"));
                        };
                        keywords.insert(
                            grant::keyword(*keyword)
                                .ok_or_else(|| unsupported("static basic keyword"))?
                                .into(),
                        );
                    }
                }
            }
        }
        Ok(keywords)
    }
}
