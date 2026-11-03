use super::super::grant::EffectiveAbility;
use super::super::{Context, Execution, InstructionOutcome, Slot, admission, player_id, seat_name};
use super::{Ability, Game, Located, Result, unsupported};
use crate::game::internal::duration::EffectEnd;
use crate::ir::ObjectRelation;
use crate::ir::prevention::{Prevention, Recipients, own_leader, projection};
use crate::ir::removal_restrictions as holder_removal;
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action, BatchPolicy, Bool,
    Control, DamageAmount, DamageFlags, DamageKind, DurationEnd, Effect, EventRewrite, ExecutionId,
    Expr, FaceId, Generation, InstallationSubjects, InstallationTime, Instruction,
    InstructionContext, InstructionId, Int, Node, ObjectId, ObjectOperand, ObjectRef, ObjectView,
    PlayerRef, Program, ProposedEventPattern, ReceiptId, RecipientSet, Requirement, Requirements,
    Selector, TimeAnchor, ZoneKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Member {
    id: String,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Kind {
    Any,
    Combat,
    Ability,
}
impl From<DamageKind> for Kind {
    fn from(kind: DamageKind) -> Self {
        match kind {
            DamageKind::Any => Self::Any,
            DamageKind::Combat => Self::Combat,
            DamageKind::Ability => Self::Ability,
        }
    }
}
impl Kind {
    fn matches(self, hit: &Value) -> bool {
        match self {
            Self::Any => true,
            Self::Combat => {
                hit["battle"] == true && !hit["target"].as_str().unwrap_or("").ends_with(".leader")
            }
            Self::Ability => hit["battle"] != true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Targets {
    Objects(Vec<Member>),
    Leaders(Vec<String>),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Rewrite {
    Prevent,
    Add(i64),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Label {
    card: String,
    name: String,
    line: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct PreventionState {
    effect: u64,
    execution: u64,
    recipients: Option<Targets>,
    sources: Option<Vec<Member>>,
    kind: Kind,
    end: EffectEnd,
    remaining: Option<u64>,
    rewrite: Rewrite,
    label: Label,
}

pub(super) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability
            .declarations
            .iter()
            .all(|effect| projection(effect).is_some())
}

fn check_equipment(shape: &Prevention<'_>) -> Result<()> {
    let host = |objects: &Selector| {
        matches!(objects, Selector::Relation {
        object: ObjectView::Current { object }, relation: ObjectRelation::Equipment
    } if object.as_ref() == &ObjectOperand::SourceHolder {})
    };
    let on_host = match (&shape.recipients, shape.damage_source) {
        (Some(Recipients::Objects(objects)), None) => host(objects),
        (None, Some(objects)) => host(objects),
        _ => false,
    };
    if !on_host
        || shape.condition != &Expr::Literal(Bool(true))
        || shape.kind != DamageKind::Any
        || !matches!(
            shape.rewrite,
            EventRewrite::AddDamage {
                amount: Expr::Literal(_)
            }
        )
    {
        return Err(unsupported("unconditional equipment host damage modifier"));
    }
    Ok(())
}

fn check_static_source(shape: &Prevention<'_>, origin: &AbilitySource) -> Result<()> {
    if shape.source != origin
        || shape.duration.end != (DurationEnd::NoTemporalExpiry {})
        || shape.duration.anchor != (TimeAnchor::Installation {})
        || shape.duration.usage.is_some()
        || shape.usage.is_some()
        || !shape.duration.intervals.is_empty()
    {
        return Err(unsupported("static damage prevention lifetime"));
    }
    Ok(())
}

pub(super) fn check(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
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
    {
        return Err(unsupported("static damage prevention header"));
    }
    for effect in &ability.declarations {
        let shape = projection(effect).ok_or_else(|| unsupported("static damage prevention"))?;
        check_static_source(&shape, &ability.source)?;
        let equipment = ability.active_zones == [ZoneKind::Equipment];
        if equipment {
            check_equipment(&shape)?;
        }
        let mut sets = Vec::new();
        if let Some(recipients) = shape.recipients {
            sets.push(match recipients {
                Recipients::Objects(objects) => RecipientSet::Objects {
                    objects: objects.clone(),
                },
                Recipients::Leaders(players) => RecipientSet::Leaders {
                    players: players.clone(),
                },
            });
        }
        if let Some(objects) = shape.damage_source {
            sets.push(RecipientSet::Objects {
                objects: objects.clone(),
            });
        }
        if equipment {
            // Both validated relation ends are public field followers, not the equipment itself.
            for set in &mut sets {
                *set = RecipientSet::Objects {
                    objects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                };
            }
        }
        let body = Program {
            scope: ability.scope,
            steps: sets
                .into_iter()
                .zip(1_u64..)
                .map(|(recipients, id)| Located {
                    source: located.source.clone(),
                    node: Node::Instruction {
                        instruction: Box::new(Instruction {
                            id: InstructionId(id),
                            actor: PlayerRef::Controller {},
                            source: ability.source.clone(),
                            precondition: None,
                            context: InstructionContext::Effect,
                            receipt: ReceiptId(id),
                            action: Action::Damage {
                                recipients,
                                amount: DamageAmount::Uniform {
                                    amount: Expr::Literal(Int(0)),
                                },
                                flags: DamageFlags::Ability,
                                source: ObjectView::SourceInformation {},
                                batch: BatchPolicy::Instruction {},
                            },
                        }),
                    },
                })
                .collect(),
        };
        let program = Program {
            scope: ability.scope,
            steps: vec![Located {
                source: located.source.clone(),
                node: Node::Control {
                    control: Control::If {
                        condition: shape.condition.clone(),
                        then_body: body,
                        else_body: None,
                    },
                },
            }],
        };
        // Reuse ordinary selector and predicate admission without executing a dummy hit.
        admission::check_card(&program, &located.source, &ability.source, None)?;
    }
    Ok(())
}

pub(in crate::game::internal) fn check_install(
    action: &Action,
    origin: &AbilitySource,
) -> Result<()> {
    let Action::InstallEffect {
        subjects,
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return Err(unsupported("damage prevention installation"));
    };
    let shape = projection(effect).ok_or_else(|| unsupported("damage prevention shape"))?;
    if !matches!(shape.rewrite, EventRewrite::PreventDamage { .. }) && shape.usage.is_none() {
        return Err(unsupported("installed numeric damage replacement"));
    }
    let captured = match subjects {
        InstallationSubjects::Objects { objects, affected } => {
            if !matches!(shape.rewrite, EventRewrite::PreventDamage { .. }) {
                return Err(unsupported("object damage prevention rewrite"));
            }
            if shape.usage.is_some()
                && !matches!(objects, Selector::Objects { objects } if objects.len() == 1)
            {
                return Err(unsupported("one next-damage recipient"));
            }
            let targets = Selector::Bound {
                reference: *affected,
            };
            match (shape.recipients, shape.damage_source) {
                (Some(Recipients::Objects(filter)), None) | (None, Some(filter)) => {
                    *filter == targets
                }
                _ => false,
            }
        }
        InstallationSubjects::Players { players, .. } => {
            shape.usage.is_some()
                && *players == own_leader()
                && matches!(shape.recipients, Some(Recipients::Leaders(expected)) if expected == players)
                && shape.damage_source.is_none()
        }
    };
    if !captured
        || shape.source != origin
        || shape.duration != duration
        || shape.condition != &Expr::Literal(Bool(true))
        || duration.anchor != (TimeAnchor::Installation {})
        || duration.usage.is_some()
        || !duration.intervals.is_empty()
        || !matches!(
            duration.end,
            DurationEnd::CurrentTurnEnd {}
                | DurationEnd::NextTurnEnd { .. }
                | DurationEnd::EndResolution {}
                | DurationEnd::NoTemporalExpiry {}
        )
        || (shape.usage.is_some() && duration.end != (DurationEnd::CurrentTurnEnd {}))
    {
        return Err(unsupported(
            "installed damage prevention capture or lifetime",
        ));
    }
    Ok(())
}

fn frozen(execution: &Execution, game: &Game, members: &[ObjectRef]) -> Result<Vec<Member>> {
    members
        .iter()
        .copied()
        .map(|member| {
            let id = execution.object_id(member)?;
            let current = game.object(id)?;
            if current.generation != member.generation.0 || current.zone != "field" {
                return Err(unsupported("non-field prevention subject"));
            }
            Ok(Member {
                id: id.into(),
                generation: member.generation.0,
            })
        })
        .collect()
}

fn source_matches(
    execution: &Execution,
    game: &Game,
    objects: &Selector,
    id: &str,
    generation: Option<u64>,
) -> Result<bool> {
    for member in execution.objects(game, objects)? {
        if execution.object_id(member)? == id {
            let generation =
                generation.ok_or_else(|| unsupported("damage source generation is unavailable"))?;
            if member.generation.0 == generation {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Prevention shares authoritative damage and duration state."
)]
impl Game {
    pub(in crate::game::internal) fn internal_install_prevention(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_install(&instruction.action, &instruction.source)?;
        let Action::InstallEffect {
            subjects,
            effect: definition,
            duration,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("damage prevention subjects"));
        };
        let shape =
            projection(definition).ok_or_else(|| unsupported("damage prevention effect"))?;
        let (members, recipients, sources, present) = match subjects {
            InstallationSubjects::Objects { objects, affected } => {
                let members = execution.objects(self, objects)?;
                let captured = frozen(execution, self, &members)?;
                execution.slots.insert(
                    affected.key(),
                    Slot::Objects {
                        members: members.clone(),
                        on_field_at_capture: true,
                    },
                );
                let recipients = shape
                    .recipients
                    .as_ref()
                    .map(|_| Targets::Objects(captured.clone()));
                let sources = shape.damage_source.map(|_| captured);
                let present = !members.is_empty();
                (members, recipients, sources, present)
            }
            InstallationSubjects::Players { players, .. } => {
                let players = execution
                    .players(self, players)?
                    .into_iter()
                    .map(|player| Ok(format!("{}.leader", seat_name(player)?)))
                    .collect::<Result<Vec<_>>>()?;
                let present = !players.is_empty();
                (vec![], Some(Targets::Leaders(players)), None, present)
            }
        };
        if present {
            let end = EffectEnd::at(self, execution, &duration.end)?;
            let label = self.prevention_label(execution)?;
            let rewrite = match shape.rewrite {
                EventRewrite::PreventDamage { .. } => Rewrite::Prevent,
                EventRewrite::AddDamage {
                    amount: Expr::Literal(amount),
                } => Rewrite::Add(amount.0),
                _ => return Err(unsupported("installed prevention rewrite")),
            };
            let effect = self.state.next_ir_effect;
            self.state.next_ir_effect = effect
                .checked_add(1)
                .ok_or_else(|| crate::invalid("prevention identities exhausted"))?;
            self.state.ir_preventions.push(PreventionState {
                effect,
                execution: execution.id.0,
                recipients,
                sources,
                kind: shape.kind.into(),
                end,
                remaining: shape.usage.map(|usage| usage.count.0),
                rewrite,
                label,
            });
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(members));
        Ok(())
    }

    fn prevention_label(&self, execution: &Execution) -> Result<Label> {
        if execution.ability.is_some() {
            let provider = self.installed_effect_provider(execution)?;
            let ordinal =
                self.printed_face_ordinal(&provider.card, &FaceId(provider.span.face.clone()))?;
            let name = self.catalog.face(&provider.card, ordinal)?["name"]
                .as_str()
                .ok_or_else(|| crate::invalid("effect provider name"))?
                .to_owned();
            return Ok(Label {
                card: provider.card,
                name,
                line: Some(u64::from(provider.span.line)),
            });
        }
        // Fragment producers have no printed ability proof; only retain an existing public face.
        let object = self
            .state
            .objects
            .get(&execution.context.source)
            .ok_or_else(|| unsupported("public prevention provider is unavailable"))?;
        let name = self.face(&object.id)?["name"]
            .as_str()
            .ok_or_else(|| crate::invalid("fragment source name"))?
            .to_owned();
        Ok(Label {
            card: object.card.clone(),
            name,
            line: None,
        })
    }

    pub(in crate::game) fn internal_prevention_remaining(&self, effect: u64) -> Result<u64> {
        self.state
            .ir_preventions
            .iter()
            .find(|state| state.effect == effect)
            .and_then(|state| state.remaining)
            .ok_or_else(|| crate::invalid("missing limited damage prevention"))
    }

    pub(in crate::game) fn internal_prevention_set_remaining(
        &mut self,
        effect: u64,
        remaining: u64,
    ) -> Result<()> {
        let state = self
            .state
            .ir_preventions
            .iter_mut()
            .find(|state| state.effect == effect)
            .ok_or_else(|| crate::invalid("missing limited damage prevention"))?;
        state.remaining = Some(remaining);
        Ok(())
    }

    pub(in crate::game) fn internal_prevention_references(
        &self,
        hit: &Value,
    ) -> Result<Vec<Value>> {
        let target = hit["target"]
            .as_str()
            .ok_or_else(|| crate::invalid("damage target"))?;
        let source = hit["source"]
            .as_str()
            .ok_or_else(|| crate::invalid("damage source"))?;
        let generation = hit["source_generation"].as_u64();
        let mut references = Vec::new();
        for effect in &self.state.ir_preventions {
            if effect.remaining == Some(0)
                || !effect.kind.matches(hit)
                || effect.end.source_expired(self)
            {
                continue;
            }
            let target_matches = match &effect.recipients {
                None => true,
                Some(Targets::Leaders(players)) => players.iter().any(|player| player == target),
                Some(Targets::Objects(objects)) => {
                    if target.ends_with(".leader") {
                        false
                    } else {
                        let current = self.object(target)?;
                        objects.iter().any(|member| {
                            member.id == target && current.generation == member.generation
                        })
                    }
                }
            };
            let source_matches = match &effect.sources {
                None => true,
                Some(objects) if objects.iter().any(|member| member.id == source) => {
                    let generation = generation
                        .ok_or_else(|| unsupported("damage source generation is unavailable"))?;
                    objects
                        .iter()
                        .any(|member| member.id == source && member.generation == generation)
                }
                Some(_) => false,
            };
            if target_matches && source_matches {
                let mut reference = json!({"reference":{"kind":"ir-damage-prevention", "effect":effect.effect},"remaining":effect.remaining});
                match effect.rewrite {
                    Rewrite::Prevent => reference["prevent"] = json!(true),
                    Rewrite::Add(amount) => reference["delta"] = json!(amount),
                }
                reference["reference"]["card"] = json!(effect.label.card);
                reference["reference"]["name"] = json!(effect.label.name);
                reference["reference"]["line"] = json!(effect.label.line);
                references.push(reference);
            }
        }
        self.static_prevention_references(hit, &mut references)?;
        Ok(references)
    }

    fn prevention_providers(&self) -> impl Iterator<Item = String> + '_ {
        self.field_ids().into_iter().chain(
            self.state
                .objects
                .values()
                .filter(|object| object.zone == "equipment")
                .map(|object| object.id.clone()),
        )
    }

    fn static_prevention_references(&self, hit: &Value, references: &mut Vec<Value>) -> Result<()> {
        let target = hit["target"]
            .as_str()
            .ok_or_else(|| crate::invalid("damage target"))?;
        let source = hit["source"]
            .as_str()
            .ok_or_else(|| crate::invalid("damage source"))?;
        for provider in self.prevention_providers() {
            let zone = if self.object(&provider)?.zone == "equipment" {
                ZoneKind::Equipment
            } else {
                ZoneKind::Field
            };
            let (card, _) = self.internal_effective_print(&provider)?;
            if !self.card_runtime.is_loaded(&card)
                && !self.internal_has_local_definitions(&provider)
            {
                continue;
            }
            let abilities = self
                .internal_effective_abilities(&provider)?
                .into_iter()
                .filter(|ability| {
                    ability.node.active_zones == [zone]
                        && (is_static(&ability.node)
                            || holder_removal::is_mixed_static(&ability.node))
                })
                .collect::<Vec<_>>();
            if abilities.is_empty() {
                continue;
            }
            let object = self.object(&provider)?;
            let execution = self.static_prevention_execution(&provider)?;
            for located in abilities {
                for effect in &located.node.declarations {
                    let Some(shape) = projection(effect) else {
                        if holder_removal::projection(effect).is_some() {
                            continue;
                        }
                        return Err(unsupported("static prevention declaration"));
                    };
                    if !Kind::from(shape.kind).matches(hit)
                        || !execution.boolean(self, shape.condition)?.0
                    {
                        continue;
                    }
                    let target_matches = match shape.recipients {
                        None => true,
                        Some(Recipients::Objects(objects)) => {
                            !target.ends_with(".leader")
                                && execution
                                    .objects(self, objects)?
                                    .contains(&execution.reference(self, target)?)
                        }
                        Some(Recipients::Leaders(players)) => {
                            match target.strip_suffix(".leader") {
                                Some(seat) => execution
                                    .players(self, players)?
                                    .contains(&player_id(seat)?),
                                None => false,
                            }
                        }
                    };
                    if !target_matches {
                        continue;
                    }
                    if let Some(objects) = shape.damage_source
                        && !source_matches(
                            &execution,
                            self,
                            objects,
                            source,
                            hit["source_generation"].as_u64(),
                        )?
                    {
                        continue;
                    }
                    let Effect::Replacement { id, .. } = effect else {
                        return Err(unsupported("prevention definition"));
                    };
                    let mut replacement = json!({"reference":{"kind":"ir-damage-prevention", "source":provider, "generation":object.generation, "definition":id.0,"line":located.source.line,"card":card,"name":self.card_name(&provider)?}});
                    self.granted_prevention_label(&located, &mut replacement)?;
                    match shape.rewrite {
                        EventRewrite::PreventDamage { .. } => {
                            replacement["prevent"] = json!(true);
                        }
                        EventRewrite::AddDamage {
                            amount: Expr::Literal(amount),
                        } => replacement["delta"] = json!(amount.0),
                        EventRewrite::CapDamage {
                            threshold: Expr::Literal(threshold),
                            replacement: Expr::Literal(amount),
                        } => replacement["cap"] = json!([threshold.0, amount.0]),
                        _ => return Err(unsupported("damage rewrite arithmetic")),
                    }
                    references.push(replacement);
                }
            }
        }
        Ok(())
    }

    fn static_prevention_execution(&self, provider: &str) -> Result<Execution> {
        let object = self.object(provider)?;
        let ids = self
            .state
            .objects
            .keys()
            .cloned()
            .zip(1_u64..)
            .map(|(id, number)| (ObjectId(number), id))
            .collect();
        self.card_execution(
            Context {
                source: provider.into(),
                generation: Generation(object.generation),
                controller: player_id(&object.controller)?,
            },
            ids,
        )
    }

    fn granted_prevention_label(
        &self,
        located: &EffectiveAbility,
        replacement: &mut Value,
    ) -> Result<()> {
        if let Some(origin) = &located.grant {
            let text = origin.provider();
            let face = self.printed_face_ordinal(&text.card, &FaceId(text.span.face.clone()))?;
            replacement["reference"]["card"] = json!(text.card);
            replacement["reference"]["name"] = self.catalog.face(&text.card, face)?["name"].clone();
            replacement["reference"]["grant"] = origin.public_stamp();
        }
        Ok(())
    }

    pub(in crate::game) fn internal_preventions_end_turn(&mut self, turn: u64) {
        self.state
            .ir_preventions
            .retain(|effect| effect.end != EffectEnd::Turn(turn));
    }
    pub(in crate::game::internal) fn internal_preventions_end_resolution(
        &mut self,
        execution: ExecutionId,
    ) {
        self.state.ir_preventions.retain(|effect| {
            effect.end != EffectEnd::EndResolution || effect.execution != execution.0
        });
    }
    pub(in crate::game) fn internal_preventions_begin_turn(&mut self) -> Result<()> {
        let turn = self.state.schedule.current()?;
        let player = player_id(self.active())?.seat;
        for effect in &mut self.state.ir_preventions {
            effect.end.begin(turn, player);
        }
        Ok(())
    }
}

pub(in crate::game::internal) fn requirement(
    requirements: &Requirements,
    requirement: &Requirement,
) -> bool {
    if !matches!(
        requirement.capability.code,
        "shape.Effect.Replacement"
            | "effect.replacement"
            | "shape.ProposedEventPattern.Damage"
            | "shape.Predicate.RecipientIsFollower"
            | "predicate.recipient-follower"
            | "shape.Predicate.RecipientIsLeader"
            | "predicate.recipient-leader"
            | "shape.Predicate.MemberOf"
            | "predicate.member-of"
            | "shape.DamageRecipient.Bound"
            | "shape.EventRewrite.PreventDamage"
            | "replacement.damage.prevent"
            | "shape.EventRewrite.AddDamage"
            | "replacement.damage.add"
            | "shape.UseLimit"
            | "shape.UsageScope.SharedPlayer"
            | "usage.shared-player"
            | "shape.EffectReference.CurrentInstallation"
            | "internal.ReplacementScope.General"
            | "internal.OncePerEvent.Enforced"
            | "internal.DamageKind.Any"
            | "internal.DamageKind.Combat"
            | "internal.DamageKind.Ability"
            | "shape.PlayerRef.ProposedDamageRecipient"
            | "player.proposed-damage-recipient"
    ) {
        return false;
    }
    let roots = requirements
        .required
        .iter()
        .filter(|item| item.capability.code == "shape.Effect.Replacement")
        .flat_map(|item| &item.locations)
        .filter(|location| location.ir_path.ends_with("/instruction/action/effect"))
        .collect::<Vec<_>>();
    // Pattern wrappers have a closed consumer; ordinary boolean expressions do not.
    requirement.locations.iter().all(|location| {
        roots.iter().any(|root| {
            location
                .ir_path
                .strip_prefix(root.ir_path.as_str())
                .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('/'))
        })
    })
}

pub(in crate::game::internal) fn mask_pattern(
    action: &mut Action,
    origin: &AbilitySource,
) -> Result<()> {
    if !matches!(&*action, Action::InstallEffect { effect, .. } if projection(effect).is_some()) {
        return Ok(());
    }
    check_install(action, origin)?;
    let Action::InstallEffect { effect, .. } = action else {
        return Err(unsupported("prevention instruction"));
    };
    let Effect::Replacement {
        pattern: ProposedEventPattern::Damage {
            recipient, source, ..
        },
        ..
    } = effect.as_mut()
    else {
        return Err(unsupported("prevention pattern"));
    };
    // Only validated event wrappers are masked; duration and instruction reads remain visible.
    if let Some(filter) = recipient {
        filter.condition = Expr::Literal(Bool(true));
    }
    if let Some(filter) = source {
        filter.condition = Expr::Literal(Bool(true));
    }
    Ok(())
}
