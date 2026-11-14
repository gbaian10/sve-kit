use super::super::grant::EffectiveAbility;
use super::super::observation::{EffectOrigin, ObjectLink};
use super::super::{Execution, InstructionOutcome, Slot, admission, player_id, seat_name};
use super::{Ability, Game, Located, Result, unsupported};
use crate::game::internal::duration::EffectEnd;
use crate::game::player::ObjectHandle;
use crate::game::player::Viewer;
use crate::game::player::observation::InstalledEffect;
use crate::game::player::observation::seat;
use crate::ir::ObjectRelation;
use crate::ir::prevention::{
    DamageSource, Prevention, Recipients, own_leader, projection, whole_turn_leader_reduction,
};
use crate::ir::removal_restrictions as holder_removal;
use crate::ir::static_declarations::{self, Family};
use crate::ir::{
    AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action, BatchPolicy, Bool,
    Control, DamageAmount, DamageFlags, DamageKind, DurationEnd, Effect, EventRewrite, ExecutionId,
    Expr, FaceId, InstallationSubjects, InstallationTime, Instruction, InstructionContext,
    InstructionId, Int, Node, ObjectOperand, ObjectRef, ObjectView, PlayerId, PlayerRef, Program,
    ProposedEventPattern, ReceiptId, RecipientSet, Requirement, Requirements, Selector, TimeAnchor,
    ZoneKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn opposing_damage_controller(hit: &Value, provider: PlayerId) -> Result<bool> {
    let controller = hit["controller"]
        .as_str()
        .ok_or_else(|| unsupported("proposed ability damage controller"))?;
    let controller = player_id(controller)
        .map_err(|_invalid_seat| unsupported("proposed ability damage controller"))?;
    Ok(controller != provider)
}

fn damage_source_matches(
    filter: Option<DamageSource<'_>>,
    execution: &Execution,
    game: &Game,
    source: &str,
    hit: &Value,
) -> Result<bool> {
    match filter {
        None => Ok(true),
        Some(DamageSource::Objects(objects)) => source_matches(
            execution,
            game,
            objects,
            source,
            hit["source_generation"].as_u64(),
        ),
        Some(DamageSource::OpponentController) => {
            opposing_damage_controller(hit, execution.context.controller)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Member {
    link: ObjectLink,
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
    origin: EffectOrigin,
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
        (None, Some(DamageSource::Objects(objects))) => host(objects),
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

// Prevention unions must not widen the shared numeric selector mask.
fn object_recipients(objects: &Selector) -> RecipientSet {
    match objects {
        Selector::Union { sets } => RecipientSet::Union {
            sets: sets.iter().map(object_recipients).collect(),
        },
        objects => RecipientSet::Objects {
            objects: objects.clone(),
        },
    }
}

pub(super) fn check(located: &Located<Ability>, declarations: &[Effect]) -> Result<()> {
    let ability = &located.node;
    if ability.kind != AbilityKind::Static
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.declarations.is_empty()
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
    for effect in declarations {
        let shape = projection(effect).ok_or_else(|| unsupported("static damage prevention"))?;
        check_static_source(&shape, &ability.source)?;
        let equipment = ability.active_zones == [ZoneKind::Equipment];
        if equipment {
            check_equipment(&shape)?;
        }
        let mut sets = Vec::new();
        if let Some(recipients) = shape.recipients {
            sets.push(match recipients {
                Recipients::Objects(objects) => object_recipients(objects),
                Recipients::Leaders(players) => RecipientSet::Leaders {
                    players: players.clone(),
                },
            });
        }
        if let Some(DamageSource::Objects(objects)) = shape.damage_source {
            sets.push(object_recipients(objects));
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
    // A whole-turn reduction on the own leader (BP14-T07) needs no use count.
    let leader_reduction = matches!(subjects, InstallationSubjects::Players { .. })
        && whole_turn_leader_reduction(&shape);
    if !matches!(shape.rewrite, EventRewrite::PreventDamage { .. })
        && shape.usage.is_none()
        && !leader_reduction
    {
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
                (Some(Recipients::Objects(filter)), None)
                | (None, Some(DamageSource::Objects(filter))) => *filter == targets,
                _ => false,
            }
        }
        InstallationSubjects::Players { players, .. } => {
            (shape.usage.is_some() || leader_reduction)
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
                link: ObjectLink::capture(game, id, member.generation.0),
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
    fn observed_prevention(
        &self,
        perspective: Viewer,
        effect: &PreventionState,
    ) -> Result<InstalledEffect> {
        use crate::game::player::observation::{
            EffectRecipient, InstalledChange, InstalledEffect, PreventionChange, viewer,
        };
        let recipients = effect
            .recipients
            .as_ref()
            .map(|targets| match targets {
                Targets::Objects(members) => Ok(members
                    .iter()
                    .filter_map(|member| self.prevention_member(perspective, member))
                    .map(EffectRecipient::Card)
                    .collect()),
                Targets::Leaders(players) => players
                    .iter()
                    .map(|name| {
                        Ok(EffectRecipient::Leader(viewer(
                            name.strip_suffix(".leader")
                                .ok_or_else(|| crate::invalid("prevention leader"))?,
                        )?))
                    })
                    .collect(),
            })
            .transpose()?;
        let sources = effect.sources.as_ref().map(|members| {
            members
                .iter()
                .filter_map(|member| self.prevention_member(perspective, member))
                .collect()
        });
        let live = !effect.end.source_expired(self);
        Ok(InstalledEffect {
            source: effect.origin.observed(self, perspective),
            expires: effect.end.observed()?,
            main_live: live,
            intervals: vec![],
            active: live && effect.remaining != Some(0),
            change: InstalledChange::Prevention {
                kind: match effect.kind {
                    Kind::Any => DamageKind::Any,
                    Kind::Combat => DamageKind::Combat,
                    Kind::Ability => DamageKind::Ability,
                },
                change: match effect.rewrite {
                    Rewrite::Prevent => PreventionChange::Prevent,
                    Rewrite::Add(amount) => PreventionChange::Add(amount),
                },
                remaining: effect.remaining,
                recipients,
                sources,
            },
        })
    }

    fn prevention_member(&self, perspective: Viewer, member: &Member) -> Option<ObjectHandle> {
        member.link.observed_public(self, perspective)
    }

    pub(in crate::game::internal) fn observed_card_preventions(
        &self,
        perspective: Viewer,
        holder: &str,
    ) -> Result<Vec<InstalledEffect>> {
        let object = self.object(holder)?;
        self.state.ir_preventions.iter().filter(|effect| {
            let matches = |members: &[Member]| members.iter().any(|member| member.id == holder && member.generation == object.generation);
            object.zone == "field" && (matches!(&effect.recipients, Some(Targets::Objects(members)) if matches(members)) || effect.sources.as_ref().is_some_and(|members| matches(members)))
        }).map(|effect| self.observed_prevention(perspective,effect)).collect()
    }

    pub(in crate::game::internal) fn observed_leader_preventions(
        &self,
        perspective: Viewer,
        player: Viewer,
    ) -> Result<Vec<InstalledEffect>> {
        self.state.ir_preventions.iter().filter(|effect| {
            let target = format!("{}.leader", seat(player));
            matches!(&effect.recipients, Some(Targets::Leaders(players)) if players.contains(&target)) && (player == perspective || effect.origin.observed(self,perspective).is_some())
        }).map(|effect| self.observed_prevention(perspective,effect)).collect()
    }

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
                origin: EffectOrigin::capture(self, execution, &instruction.source)?,
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
                        && static_declarations::contains(&ability.node, Family::Prevention)
                })
                .collect::<Vec<_>>();
            if abilities.is_empty() {
                continue;
            }
            let object = self.object(&provider)?;
            let execution = self.provider_execution(&provider, None)?;
            for located in abilities {
                for effect in &located.node.declarations {
                    let Some(shape) = projection(effect) else {
                        if static_declarations::family(effect).is_some()
                            || holder_removal::projection(effect).is_some()
                        {
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
                    if !damage_source_matches(shape.damage_source, &execution, self, source, hit)? {
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

#[cfg(test)]
mod tests;

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
            | "shape.PlayerRef.ProposedDamageController"
            | "player.proposed-damage-controller"
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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Replacement choices share captured prevention providers."
)]
impl Game {
    pub(in crate::game::internal) fn player_replacement_link(
        &self,
        reference: &Value,
    ) -> Option<ObjectLink> {
        if let Some(id) = reference["effect"].as_u64() {
            return self
                .state
                .ir_preventions
                .iter()
                .find(|effect| effect.effect == id)
                .map(|effect| effect.origin.holder_link());
        }
        Some(ObjectLink::capture(
            self,
            reference["source"].as_str()?,
            reference["generation"].as_u64()?,
        ))
    }
}
