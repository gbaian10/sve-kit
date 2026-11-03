use super::super::{Context, admission, grant, player_id};
use super::{Ability, Game, Located, Result, unsupported};
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, Action, Bool, CharacteristicPatch,
    ContinuousLayer, Control, Duration, DurationEnd, Effect, EffectTarget, Expr, FaceId,
    Generation, GrantedAbility, Instruction, InstructionContext, InstructionId, KeywordId, Node,
    ObjectId, ObjectOperand, ObjectRelation, ObjectView, PlayerRef, PostureRead, Predicate,
    Program, ReceiptId, Requirements, Selector, TextProvider, TimeAnchor, TimestampBasis, ZoneKind,
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

fn grant_subjects(
    ability: &Ability,
    selector: &Selector,
    condition: Option<&Expr<Bool>>,
) -> Result<Selector> {
    Ok(if ability.active_zones == [ZoneKind::Equipment] {
        if condition.is_some()
            || !matches!(selector, Selector::Relation {
                object: ObjectView::Current { object }, relation: ObjectRelation::Equipment
            } if object.as_ref() == &ObjectOperand::SourceHolder {})
        {
            return Err(unsupported("equipment grants to its associated follower"));
        }
        // The scoped host relation reads only the public field endpoint.
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        }
    } else {
        selector.clone()
    })
}

fn check_header(ability: &Ability) -> Result<()> {
    if !is_static(ability)
        || !matches!(
            ability.active_zones.as_slice(),
            [ZoneKind::Field | ZoneKind::Equipment]
        )
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
    Ok(())
}

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    check_header(ability)?;
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
            || active_zones != &ability.active_zones
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
        let subjects = grant_subjects(ability, selector, condition.as_ref())?;
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
                            subjects,
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
                        condition: scoped_condition(condition.as_ref()),
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
        self.internal_static_keyword_grants(holder)?
            .into_iter()
            .map(|(keyword, _)| {
                grant::keyword(keyword)
                    .map(str::to_owned)
                    .ok_or_else(|| unsupported("static keyword"))
            })
            .collect()
    }

    pub(in crate::game::internal) fn internal_static_keyword_grants(
        &self,
        holder: &str,
    ) -> Result<Vec<(KeywordId, grant::GrantOrigin)>> {
        Ok(self
            .internal_static_grants(holder)?
            .into_iter()
            .filter_map(|(ability, origin)| match ability {
                GrantedAbility::Keyword { keyword } => Some((keyword, origin)),
                GrantedAbility::Definition { .. } => None,
            })
            .collect())
    }

    pub(in crate::game::internal) fn internal_static_grants(
        &self,
        holder: &str,
    ) -> Result<Vec<(GrantedAbility, grant::GrantOrigin)>> {
        let recipient = self.object(holder)?;
        let mut keywords = Vec::new();
        if recipient.zone != "field" {
            return Ok(keywords);
        }
        let providers = self.field_ids().into_iter().chain(
            self.state
                .objects
                .values()
                .filter(|object| object.zone == "equipment")
                .map(|object| object.id.clone()),
        );
        for source in providers {
            let zone = if self.object(&source)?.zone == "equipment" {
                ZoneKind::Equipment
            } else {
                ZoneKind::Field
            };
            let (card, _) = self.internal_effective_print(&source)?;
            if !self.card_runtime.is_loaded(&card) && !self.internal_has_local_definitions(&source)
            {
                continue;
            }
            let static_abilities = self
                .internal_local_abilities(&source)?
                .into_iter()
                .filter(|located| is_static(&located.node) && located.node.active_zones == [zone])
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
                        id,
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
                        keywords.push((
                            ability.clone(),
                            self.internal_static_origin(&source, &located, id.0)?,
                        ));
                    }
                }
            }
        }
        Ok(keywords)
    }
}

fn scoped_condition(condition: Option<&Expr<Bool>>) -> Expr<Bool> {
    condition.map_or(Expr::Literal(Bool(true)), field_holder_posture)
}

// A field-only static's holder is public, so its own posture is a scoped live holder read.
pub(super) fn field_holder_posture(condition: &Expr<Bool>) -> Expr<Bool> {
    let Expr::Compute(operation) = condition else {
        return condition.clone();
    };
    match operation.as_ref() {
        Predicate::PostureEquals {
            left: Expr::Live(read),
            right: Expr::Literal(_),
        } if matches!(read.as_ref(), PostureRead::Posture { object: ObjectView::Current { object } } if matches!(object.as_ref(), ObjectOperand::SourceHolder {})) => {
            Expr::Literal(Bool(true))
        }
        Predicate::And { predicates } => Expr::Compute(Box::new(Predicate::And {
            predicates: predicates.iter().map(field_holder_posture).collect(),
        })),
        Predicate::Or { predicates } => Expr::Compute(Box::new(Predicate::Or {
            predicates: predicates.iter().map(field_holder_posture).collect(),
        })),
        Predicate::Not { predicate } => Expr::Compute(Box::new(Predicate::Not {
            predicate: field_holder_posture(predicate),
        })),
        _ => condition.clone(),
    }
}
