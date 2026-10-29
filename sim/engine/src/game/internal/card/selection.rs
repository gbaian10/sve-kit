use super::super::{DecisionToken, DecisionView, InputDomain, Slot, Wait, contains};
use super::{Execution, Game, Nat, ObjectRef, Result, invalid, player_id, unsupported};
use crate::ir::effect_evolution;
use crate::ir::{
    Binding, CardKind, Cardinality, CharacteristicView, ChoiceDomain, Decision, DecisionKind,
    Disjointness, KindRead, NamesRead, ObjectOperand, ObjectProperty, ObjectView, PlayerRef,
    PlayerRelation, Predicate, ScopeId, SelectionConstraint, SelectionGroup, Selector, TraitsRead,
};
use crate::ir::{Bool, Expr, SelectionView, ZoneKind};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use serde_json::{Value, json};

/// Object candidates visible only to the current chooser, addressed within that question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetDomain {
    /// Token-scoped objects; a discard may show the chooser's own hand cards.
    pub members: Vec<super::super::DistributionMember>,
    /// Bounds for this question; play-time targets retain their strict printed minimum.
    pub cardinality: Cardinality,
    /// Selected objects must not share a rules name.
    pub distinct_names: bool,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct TargetWait {
    pub(in crate::game::internal) decision: Decision,
    binding: Binding<Selector>,
    members: Vec<ObjectRef>,
    bounds: Cardinality,
    characteristics: Vec<(ObjectRef, TargetCharacteristics)>,
    distinct_names: bool,
    on_field_at_capture: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::game::internal) struct TargetCharacteristics {
    controller: String,
    zone: String,
    kind: String,
    traits: Vec<String>,
    names: Vec<String>,
}

pub(in crate::game::internal) fn characteristics(
    game: &Game,
    id: &str,
) -> Result<TargetCharacteristics> {
    // The admitted closure has no trait patches, static effects or base replacement during play.
    let traits = game.face(id)?["traits"]
        .as_array()
        .ok_or_else(|| invalid("missing effective traits"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("invalid effective trait"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(TargetCharacteristics {
        names: game.card_names(id)?,
        controller: game.object(id)?.controller.clone(),
        zone: game.object(id)?.zone.clone(),
        kind: game.object_type(id)?.into(),
        traits,
    })
}

impl TargetWait {
    pub(in crate::game::internal) fn check_current(
        &self,
        game: &Game,
        execution: &Execution,
        members: &[ObjectRef],
    ) -> Result<()> {
        for member in members {
            let id = execution.object_id(*member)?;
            let current = game.object(id)?;
            if current.generation != member.generation.0 {
                continue;
            }
            let saved = self
                .characteristics
                .iter()
                .find(|(reference, _)| reference == member)
                .ok_or_else(|| invalid("missing target selection producer"))?;
            if characteristics(game, id)? != saved.1 {
                return Err(unsupported("selected target characteristics changed"));
            }
        }
        Ok(())
    }

    pub(in crate::game::internal) fn validate(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        if !contains(
            self.bounds,
            Nat(u64::try_from(handles.len()).map_err(invalid)?),
        ) {
            return Err(invalid("target cardinality"));
        }
        let mut unique = BTreeSet::new();
        let members = handles
            .iter()
            .map(|handle| {
                if !unique.insert(*handle) {
                    return Err(invalid("duplicate target handle"));
                }
                self.members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("foreign target handle"))
            })
            .collect::<Result<Vec<_>>>()?;
        if !self.compatible(&members)? {
            return Err(invalid("target names must differ"));
        }
        Ok(members)
    }

    fn compatible(&self, members: &[ObjectRef]) -> Result<bool> {
        if !self.distinct_names {
            return Ok(true);
        }
        let mut names = BTreeSet::new();
        for member in members {
            let saved = self
                .characteristics
                .iter()
                .find(|(reference, _)| reference == member)
                .ok_or_else(|| invalid("missing target names"))?;
            let own: BTreeSet<_> = saved.1.names.iter().collect();
            if own.iter().any(|name| names.contains(name)) {
                return Ok(false);
            }
            names.extend(own);
        }
        Ok(true)
    }

    fn capacity(&self) -> Result<u64> {
        if !self.distinct_names {
            return u64::try_from(self.members.len()).map_err(invalid);
        }
        if self.members.len() > 10 {
            return Err(unsupported("name selection exceeds both fields"));
        }
        let end = 1_u64
            .checked_shl(u32::try_from(self.members.len()).map_err(invalid)?)
            .ok_or_else(|| invalid("target plan overflow"))?;
        let mut maximum = 0;
        for mask in 0..end {
            let members = self
                .members
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_u64 << index) != 0)
                .map(|(_, member)| *member)
                .collect::<Vec<_>>();
            if self.compatible(&members)? {
                maximum = maximum.max(u64::try_from(members.len()).map_err(invalid)?);
            }
        }
        Ok(maximum)
    }
}

fn target_group(constraint: &SelectionConstraint) -> Result<&SelectionGroup> {
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("one selector group per target decision"));
    };
    if !matches!(constraint.distinct_by, None | Some(ObjectProperty::Names))
        || !constraint.aggregate_bounds.is_empty()
        || constraint.disjoint != Disjointness::Disjoint
    {
        return Err(unsupported("target properties or aggregate constraints"));
    }
    Ok(group)
}

pub(super) fn check(constraint: &SelectionConstraint) -> Result<()> {
    field_selector(&target_group(constraint)?.selector)
}

pub(in crate::game::internal) fn check_resolution(decision: &Decision) -> Result<()> {
    if effect_evolution::is_choice(decision) {
        return Ok(());
    }
    if let ChoiceDomain::Recipients {
        candidates,
        cardinality,
        ..
    } = &decision.domain
    {
        if decision.actor != (PlayerRef::Controller {}) {
            return Err(unsupported("resolution recipient actor"));
        }
        return super::recipient_targets::check(candidates, *cardinality);
    }
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return Err(unsupported("resolution public choice domain"));
    };
    let group = target_group(constraint)?;
    match &group.selector {
        Selector::Zone {
            zone: ZoneKind::Ex,
            view: SelectionView::Current {},
            players,
            binding,
            predicate,
        } if players.relative_to == (PlayerRef::Controller {}) => {
            check_predicate(predicate, *binding)?;
        }
        Selector::Zone {
            zone: ZoneKind::Cemetery,
            view: SelectionView::Current {},
            players,
            binding,
            predicate,
        } if players.relative_to == (PlayerRef::Controller {})
            && players.relation == PlayerRelation::SelfPlayer =>
        {
            check_predicate(predicate, *binding)?;
        }
        selector => field_selector(selector)?,
    }
    let selector = match &group.selector {
        Selector::Difference { input, .. } => input.as_ref(),
        selector => selector,
    };
    if decision.actor != (PlayerRef::Controller {})
        || constraint.cardinality.maximum.is_none()
        || !matches!(
            selector,
            Selector::Zone {
                zone: ZoneKind::Field | ZoneKind::Ex | ZoneKind::Cemetery,
                ..
            }
        )
    {
        return Err(unsupported("resolution public choice actor or selector"));
    }
    Ok(())
}

pub(in crate::game::internal) fn field_selector(selector: &Selector) -> Result<()> {
    match selector {
        Selector::Zone {
            zone: ZoneKind::Field,
            view: SelectionView::Current {},
            predicate,
            binding,
            players,
        } if players.relative_to == (PlayerRef::Controller {}) => {
            check_predicate(predicate, *binding)
        }
        Selector::Difference { input, except } if matches!(except.as_ref(), Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}]) => {
            field_selector(input)
        }
        Selector::Captured { .. } | Selector::Bound { .. } | Selector::Objects { .. } => Ok(()),
        Selector::Union { sets } => {
            for set in sets {
                field_selector(set)?;
            }
            Ok(())
        }
        _ => Err(unsupported("public field target selector")),
    }
}

fn candidate_view(view: &ObjectView, candidate: Binding<ObjectRef>) -> bool {
    matches!(view, ObjectView::Current { object } if matches!(object.as_ref(), ObjectOperand::Bound { binding } if *binding == candidate))
}

fn check_predicate(value: &Expr<Bool>, candidate: Binding<ObjectRef>) -> Result<()> {
    match value {
        Expr::Literal(Bool(true)) => Ok(()),
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::KindEquals {
                left: Expr::Live(read),
                right: Expr::Literal(_),
            } if matches!(read.as_ref(), KindRead::Kind { object, view: CharacteristicView::Effective } if candidate_view(object, candidate)) => {
                Ok(())
            }
            Predicate::NamesIntersect {
                left: Expr::Live(read),
                right: Expr::Literal(names),
            } if !names.is_empty()
                && names.iter().all(|name| !name.0.is_empty())
                && matches!(read.as_ref(), NamesRead::Names { object } if candidate_view(object, candidate)) =>
            {
                Ok(())
            }
            Predicate::NameContains {
                object,
                name: Expr::Literal(name),
            } if !name.0.is_empty() && candidate_view(object, candidate) => Ok(()),
            Predicate::HasTrait {
                traits: Expr::Live(read),
                trait_id,
            } if !trait_id.0.is_empty()
                && matches!(read.as_ref(), TraitsRead::Traits { object, view: CharacteristicView::Effective } if candidate_view(object, candidate)) =>
            {
                Ok(())
            }
            Predicate::And { predicates } | Predicate::Or { predicates }
                if !predicates.is_empty() =>
            {
                for predicate in predicates {
                    check_predicate(predicate, candidate)?;
                }
                Ok(())
            }
            Predicate::Not { predicate } => check_predicate(predicate, candidate),
            _ => Err(unsupported(
                "unstable or unsupported field target predicate",
            )),
        },
        _ => Err(unsupported("field target predicate")),
    }
}

pub(super) fn matches_candidate(game: &Game, id: &str, value: &Expr<Bool>) -> Result<bool> {
    match value {
        Expr::Literal(Bool(true)) => Ok(true),
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::KindEquals {
                right: Expr::Literal(kind),
                ..
            } => Ok(game.object_type(id)?.contains(match kind {
                CardKind::Follower => "フォロワー",
                CardKind::Spell => "スペル",
                CardKind::Amulet => "アミュレット",
            })),
            Predicate::NamesIntersect {
                right: Expr::Literal(names),
                ..
            } => Ok(game
                .card_names(id)?
                .iter()
                .any(|actual| names.iter().any(|name| &name.0 == actual))),
            Predicate::NameContains {
                name: Expr::Literal(name),
                ..
            } => Ok(game
                .card_names(id)?
                .iter()
                .any(|actual| actual.contains(&name.0))),
            Predicate::HasTrait { trait_id, .. } => {
                Ok(characteristics(game, id)?.traits.contains(&trait_id.0))
            }
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                let values = predicates
                    .iter()
                    .map(|predicate| matches_candidate(game, id, predicate))
                    .collect::<Result<Vec<_>>>()?;
                Ok(if matches!(operation.as_ref(), Predicate::And { .. }) {
                    values.iter().all(|matched| *matched)
                } else {
                    values.iter().any(|matched| *matched)
                })
            }
            Predicate::Not { predicate } => Ok(!matches_candidate(game, id, predicate)?),
            _ => Err(unsupported("field target predicate evaluation")),
        },
        _ => Err(unsupported("field target predicate evaluation")),
    }
}

pub(in crate::game::internal) fn packet(domain: &TargetDomain) -> Value {
    json!({"kind":"targets","distinct_names":domain.distinct_names,"minimum":domain.cardinality.minimum.0,"maximum":domain.cardinality.maximum.map(|n| n.0),"members":domain.members.iter().map(|member| json!({"handle":member.handle,"player":member.player.seat,"card":member.card,"name":member.name})).collect::<Vec<_>>()})
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Target evaluation uses the play's frozen authority view."
)]
impl Game {
    pub(super) fn internal_target(
        &self,
        execution: &mut Execution,
        decision: &Decision,
    ) -> Result<()> {
        let (wait, domain) = self.target_domain(execution, decision)?;
        if wait.bounds.minimum.0 > wait.capacity()? {
            return Err(crate::EngineFailure::IllegalPlay(
                "no legal play targets".into(),
            ));
        }
        execution.waiting = Some((
            DecisionView {
                token: DecisionToken {
                    execution: execution.id,
                    decision: decision.id,
                    epoch: 0,
                },
                actor: execution.context.controller,
                domain: InputDomain::Targets(domain),
            },
            Wait::Targets(wait),
        ));
        Ok(())
    }

    pub(in crate::game::internal) fn internal_resolution_target(
        &mut self,
        execution: &mut Execution,
        decision: &Decision,
    ) -> Result<()> {
        check_resolution(decision)?;
        let (mut wait, mut domain) = self.target_domain(execution, decision)?;
        let available = wait.capacity()?;
        // CR 1.3.2: a resolving instruction does as much as possible; play stays strict.
        wait.bounds.minimum = Nat(wait.bounds.minimum.0.min(available));
        domain.cardinality = wait.bounds;
        let actor = execution.player(self, &decision.actor)?;
        // Even an empty or singleton resolution domain remains a player's decision.
        self.internal_wait(
            execution,
            decision.id,
            actor,
            InputDomain::Targets(domain),
            Wait::ResolutionTargets(wait),
        )
    }

    fn target_domain(
        &self,
        execution: &Execution,
        decision: &Decision,
    ) -> Result<(TargetWait, TargetDomain)> {
        let ChoiceDomain::Objects {
            constraint,
            binding,
            ..
        } = &decision.domain
        else {
            return Err(unsupported("target domain"));
        };
        let [group] = constraint.groups.as_slice() else {
            return Err(unsupported("target groups"));
        };
        let allow_resolution_zones = matches!(decision.kind, DecisionKind::ResolutionChoice { .. });
        let evolution = effect_evolution::is_choice(decision);
        let mut members = Vec::new();
        let candidates = if evolution {
            self.effect_evolution_members(execution)?
        } else {
            execution.public_target_objects(self, &group.selector, allow_resolution_zones)?
        };
        for member in candidates {
            if evolution
                || self.targetable_by(
                    execution.object_id(member)?,
                    super::seat_name(execution.context.controller)?,
                )?
            {
                members.push(member);
            }
        }
        let bounds = Cardinality {
            minimum: Nat(constraint
                .cardinality
                .minimum
                .0
                .max(group.cardinality.minimum.0)),
            maximum: constraint
                .cardinality
                .maximum
                .into_iter()
                .chain(group.cardinality.maximum)
                .min_by_key(|n| n.0),
        };
        let public = members
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let id = execution.object_id(*reference)?;
                let object = self.object(id)?;
                let kind = self.object_type(id)?;
                if !(object.zone == "field"
                    || (evolution && object.zone == "evolve_deck")
                    || (allow_resolution_zones
                        && matches!(object.zone.as_str(), "ex" | "cemetery")))
                    || object.generation != reference.generation.0
                    || (object.zone == "field"
                        && !(kind.contains("フォロワー") || kind.contains("アミュレット")))
                {
                    return Err(unsupported(
                        "target is not a current public field object or EX card or cemetery card",
                    ));
                }
                if !evolution && object.zone != "cemetery" && self.restricted(id, "choose")? {
                    return Err(unsupported("target permission"));
                }
                Ok(super::super::DistributionMember {
                    handle: u64::try_from(index).map_err(invalid)?,
                    player: player_id(&object.controller)?,
                    card: Some(object.card.clone()),
                    name: Some(self.card_name(id)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let characteristics = members
            .iter()
            .map(|reference| {
                Ok((
                    *reference,
                    characteristics(self, execution.object_id(*reference)?)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let on_field_at_capture = characteristics
            .iter()
            .all(|(_, saved)| saved.zone == "field");
        Ok((
            TargetWait {
                decision: decision.clone(),
                binding: *binding,
                members,
                bounds,
                characteristics,
                distinct_names: constraint.distinct_by == Some(ObjectProperty::Names),
                on_field_at_capture,
            },
            TargetDomain {
                members: public,
                cardinality: bounds,
                distinct_names: constraint.distinct_by == Some(ObjectProperty::Names),
            },
        ))
    }

    pub(super) fn internal_mode_targets_feasible(
        &self,
        execution: &Execution,
        parameters: &[Decision],
    ) -> Result<bool> {
        for decision in parameters {
            let (wait, _) = self.target_domain(execution, decision)?;
            if wait.bounds.minimum.0 > wait.capacity()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn internal_feasible_targets(
        &self,
        execution: &Execution,
        slots: &BTreeMap<(ScopeId, u64), Slot>,
        parameters: &VecDeque<Decision>,
    ) -> Result<bool> {
        if parameters.is_empty() {
            return Ok(true);
        }
        let mut candidate = execution.clone();
        candidate.slots.clone_from(slots);
        let first = parameters
            .front()
            .ok_or_else(|| invalid("missing target restart"))?;
        if let ChoiceDomain::Distribution {
            recipients, total, ..
        } = &first.domain
        {
            let recipients = candidate.recipients(self, recipients)?;
            let total = candidate.natural(self, total)?;
            if !recipients.members().is_empty()
                && u64::try_from(recipients.members().len()).map_err(invalid)? > total.0
            {
                return Ok(false);
            }
            candidate.captures(self, &first.captures)?;
            return self.internal_feasible_targets(
                &candidate,
                &candidate.slots,
                &parameters.iter().skip(1).cloned().collect(),
            );
        }
        if matches!(first.domain, ChoiceDomain::Recipients { .. }) {
            let (wait, _) = self.recipient_target_domain(&candidate, first)?;
            for members in wait.selections()? {
                let mut next = candidate.clone();
                next.slots.insert(
                    wait.binding.key(),
                    Slot::Recipients {
                        members,
                        characteristics: wait.characteristics.clone(),
                    },
                );
                if self.internal_feasible_targets(
                    &next,
                    &next.slots,
                    &parameters.iter().skip(1).cloned().collect(),
                )? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let (wait, _) = self.target_domain(&candidate, first)?;
        // Both public fields have a rules-defined capacity of five, so this proof is finite.
        if wait.members.len() > 10 {
            return Err(unsupported("target feasibility exceeds both fields"));
        }
        let end = 1_u64
            .checked_shl(u32::try_from(wait.members.len()).map_err(invalid)?)
            .ok_or_else(|| invalid("target plan overflow"))?;
        for mask in 0..end {
            let members = wait
                .members
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_u64 << index) != 0)
                .map(|(_, member)| *member)
                .collect::<Vec<_>>();
            if !contains(
                wait.bounds,
                Nat(u64::try_from(members.len()).map_err(invalid)?),
            ) || !wait.compatible(&members)?
            {
                continue;
            }
            let mut candidate = candidate.clone();
            candidate.bind_targets(&wait, members);
            candidate.captures(self, &first.captures)?;
            if self.internal_feasible_targets(
                &candidate,
                &candidate.slots,
                &parameters.iter().skip(1).cloned().collect(),
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Selected targets retain a checked producer separate from installation locals."
)]
impl Execution {
    pub(in crate::game::internal) fn public_field_objects(
        &self,
        game: &Game,
        selector: &Selector,
    ) -> Result<Vec<ObjectRef>> {
        self.public_target_objects(game, selector, false)
    }

    fn public_target_objects(
        &self,
        game: &Game,
        selector: &Selector,
        allow_resolution_zones: bool,
    ) -> Result<Vec<ObjectRef>> {
        let members = match selector {
            Selector::Zone {
                players,
                binding,
                predicate,
                zone,
                ..
            } => {
                let zone = match zone {
                    ZoneKind::Field => "field",
                    ZoneKind::Ex if allow_resolution_zones => "ex",
                    ZoneKind::Cemetery if allow_resolution_zones => "cemetery",
                    _ => return Err(unsupported("public target zone")),
                };
                check_predicate(predicate, *binding)?;
                let mut members = Vec::new();
                for player in self.players(game, players)? {
                    for id in game.zone_ids(super::super::seat_name(player)?, zone) {
                        if matches_candidate(game, &id, predicate)? {
                            members.push(self.reference(game, &id)?);
                        }
                    }
                }
                members
            }
            Selector::Union { sets } => sets
                .iter()
                .map(|set| self.public_target_objects(game, set, allow_resolution_zones))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect(),
            _ => self.objects(game, selector)?,
        };
        let mut seen = BTreeSet::new();
        Ok(members
            .into_iter()
            .filter(|member| seen.insert((member.id, member.generation)))
            .collect())
    }

    pub(in crate::game::internal) fn bind_targets(
        &mut self,
        wait: &TargetWait,
        members: Vec<ObjectRef>,
    ) {
        self.targets.insert(wait.binding.key(), wait.clone());
        self.slots.insert(
            wait.binding.key(),
            Slot::Objects {
                members,
                on_field_at_capture: wait.on_field_at_capture,
            },
        );
    }
}
