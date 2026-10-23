use super::super::{Answer, DecisionToken, DecisionView, InputDomain, Slot, Wait};
use super::{
    Ability, AbilityKind, Activation, Context, Execution, ExecutionId, Expr, Fact, Game,
    Generation, Int, Located, Nat, Node, PlayerId, Result, ResumeStage, RuntimeAbilitySource,
    SourceSpan, Step, admission, face_ordinal, int, invalid, printed_origin, seat_name,
    unsupported,
};
use crate::game::legal::subsets;
use crate::ir::quick::is_printed_quick;
use crate::ir::{
    Action, Cardinality, LimitSpec, ModeUse, ObjectRef, PlaySpec, PlayerRef, RecipientSet,
    SelectionSnapshot, Selector, TurnId,
};
use crate::ir::{
    ChoiceDomain, Control, CostSpec, Decision, DecisionId, DecisionKind, ExecutionRef, InputMode,
    Mode, ModeChoiceKind, ModeDecision, ModeDecisionKind, ModeId, PlaySlot, Program, ScopeId,
    SnapshotId, Visibility,
};
use alloc::collections::{BTreeMap, VecDeque};
use core::slice::from_ref;
use serde_json::{Value, json};

/// A formal play exposes a question or a fully committed result, never partial receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardPlay {
    /// Answer through `answer_internal_card` using only this token and a payload.
    Awaiting(DecisionView),
    /// Complete printed Spell or ability, followed by explicit check timing.
    Resolved(Activation),
    /// Complete follower or amulet play, followed by its separately produced fanfare.
    Entered(CardEntry),
    /// One pending occurrence cannot legally be played and has been cancelled by rules.
    Cancelled,
    /// A valid Quick Pass resumed the stored attack or end-phase procedure.
    Continued(Step),
}

/// Complete follower or amulet rule play; fanfare remains a separate execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardEntry {
    /// Authority producer for the card play.
    pub execution: ExecutionId,
    /// Complete intrinsic payment and card facts.
    pub progress: super::super::Progress,
    /// Committed causal movement and play events.
    pub step: Step,
    /// The authority-produced fanfare follows the completed rule check.
    pub resume: ResumeStage,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct ModeWait {
    pub(in crate::game::internal) chosen: ModeDecision,
    pub(in crate::game::internal) count: Nat,
}

#[derive(Clone, Debug)]
struct PlayMode {
    modes: Vec<Mode>,
    count: Expr<Nat>,
    chosen: ModeDecision,
}

impl PlayMode {
    fn parameters(&self) -> Result<Vec<Decision>> {
        let mut parameters = Vec::new();
        if matches!(
            &self.chosen.domain,
            ChoiceDomain::Modes {
                cardinality: Cardinality {
                    minimum: Nat(0),
                    ..
                },
                ..
            }
        ) && self.count != Expr::Literal(Nat(0))
        {
            return Err(unsupported("any-number mode choice"));
        }
        for mode in &self.modes {
            if mode.parameters.iter().any(|decision| !decision.captures.is_empty()
                || !matches!(&decision.domain, ChoiceDomain::Objects { constraint, .. }
                    if constraint.groups.iter().all(|target| independent_selector(&target.selector)))) {
                return Err(unsupported("mode targets require independent public selectors"));
            }
            parameters.extend(mode.parameters.clone());
        }
        Ok(parameters)
    }
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct PlayState {
    snapshot: Option<Box<Game>>,
    pub(in crate::game::internal) snapshot_id: SnapshotId,
    modes: VecDeque<PlayMode>,
    groups: Vec<PlayMode>,
    pub(in crate::game::internal) chosen: BTreeMap<DecisionId, Vec<ModeId>>,
    parameters: VecDeque<Decision>,
    pub(super) exchange: Option<super::exchange::ExchangeState>,
    pub(super) costs: CostSpec,
    limit: Option<LimitSpec>,
    body: Program,
    restart: Option<SelectionOrigin>,
    distribution_captures: Option<Decision>,
}

#[derive(Clone, Debug)]
struct SelectionOrigin {
    slots: BTreeMap<(ScopeId, u64), Slot>,
    parameters: VecDeque<Decision>,
}

impl PlayState {
    fn freeze_selection(&mut self, game: &Game, execution: &Execution) -> Result<()> {
        if self.snapshot.is_none() {
            for group in &self.groups {
                let chosen = self
                    .chosen
                    .get(&group.chosen.id)
                    .ok_or_else(|| invalid("missing modes"))?;
                for mode in &group.modes {
                    if chosen.contains(&mode.id) {
                        self.parameters.extend(mode.parameters.clone());
                    }
                }
            }
            // The first target fixes the game view; later answers only extend immutable slots.
            self.snapshot_id = SnapshotId(game.state.ir_epoch);
            self.snapshot = Some(Box::new(game.clone()));
            self.restart = Some(SelectionOrigin {
                slots: execution.slots.clone(),
                parameters: self.parameters.clone(),
            });
            // Never ask a target question that no target and distribution plan can satisfy.
            if self
                .parameters
                .iter()
                .any(|decision| matches!(decision.domain, ChoiceDomain::Distribution { .. }))
                && !game.internal_feasible_targets(execution, &execution.slots, &self.parameters)?
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "no legal target and distribution plan".into(),
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn check_spell(ability: &Ability, printed: &Value) -> Result<()> {
    let Some(play) = &ability.play else {
        return Err(unsupported("Spell play"));
    };
    let CostSpec::Ordered { steps } = &play.costs else {
        return Err(unsupported("Spell costs"));
    };
    let [step] = steps.as_slice() else {
        return Err(unsupported("Spell needs one intrinsic PP payment"));
    };
    let Action::Resource {
        amount: Expr::Literal(Int(amount)),
        ..
    } = &step.instruction.action
    else {
        return Err(unsupported("Spell intrinsic cost"));
    };
    if printed["card_type"] != "スペル"
        || *amount != super::super::cost::printed_play_cost(&printed["cost"])?
    {
        return Err(unsupported(
            "Spell catalogue kind or intrinsic cost mismatch",
        ));
    }
    Ok(())
}

pub(super) fn check(ability: &Ability, source: &SourceSpan) -> Result<()> {
    let Some(play) = &ability.play else {
        return Ok(());
    };
    let body = ability
        .body
        .as_ref()
        .ok_or_else(|| invalid("missing body"))?;
    let groups = collect_modes(body)?;
    if is_printed_quick(ability)
        && groups.iter().any(|group| {
            matches!(
                &group.chosen.domain,
                ChoiceDomain::Modes {
                    exclude_used: Some(_),
                    ..
                }
            )
        })
    {
        return Err(unsupported(
            "Quick mode exclusion needs a history-aware preflight",
        ));
    }
    let mut parameters = play.parameters.clone();
    for group in &groups {
        parameters.extend(group.parameters()?);
    }
    let mut distribution = false;
    let mut target = None;
    let mut target_count = 0_usize;
    for decision in &parameters {
        match (&decision.kind, &decision.domain) {
            (
                DecisionKind::PlayParameter {
                    slot: PlaySlot::Targets,
                    selection_snapshot: SelectionSnapshot::AtDecision,
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                },
                ChoiceDomain::Objects {
                    constraint,
                    binding,
                    target_roles: None,
                },
            ) if !distribution => {
                super::selection::check(constraint)?;
                target = Some(*binding);
                target_count = target_count.saturating_add(1);
            }
            (
                DecisionKind::PlayParameter {
                    slot: PlaySlot::Distribution,
                    selection_snapshot: SelectionSnapshot::AtDecision,
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                },
                ChoiceDomain::Distribution {
                    recipients:
                        RecipientSet::Objects {
                            objects: Selector::Bound { reference },
                        },
                    minimum: Nat(1),
                    ..
                },
            ) if Some(*reference) == target => {
                distribution = true;
            }
            _ => return Err(unsupported("play parameter order, dependencies or domain")),
        }
        if decision.actor != (PlayerRef::Controller {})
            || decision.visibility != (Visibility::Public {})
        {
            return Err(unsupported("play parameter actor or visibility"));
        }
    }
    if distribution && target_count != 1 {
        return Err(unsupported("distribution needs exactly one target group"));
    }
    let mut steps = play
        .captures
        .iter()
        .cloned()
        .map(|capture| Located {
            node: Node::Capture { capture },
            source: source.clone(),
        })
        .collect::<Vec<_>>();
    steps.extend(parameters.into_iter().map(|decision| Located {
        node: Node::Decision { decision },
        source: source.clone(),
    }));
    admission::check_play(
        &Program {
            scope: ability.scope,
            steps,
        },
        source,
    )
}

fn collect_modes(program: &Program) -> Result<Vec<PlayMode>> {
    let mut groups = Vec::new();
    for node in &program.steps {
        match &node.node {
            Node::Control {
                control: Control::Sequence { body },
            } => groups.extend(collect_modes(body)?),
            Node::Control {
                control:
                    Control::Mode {
                        modes,
                        count,
                        chosen,
                        choice_kind: ModeChoiceKind::ChoiceKeyword,
                    },
            } if matches!(
                chosen.kind,
                ModeDecisionKind::PlayParameter {
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline
                }
            ) && chosen.actor == (PlayerRef::Controller {}) =>
            {
                groups.push(PlayMode {
                    modes: modes.clone(),
                    count: count.clone(),
                    chosen: chosen.clone(),
                });
            }
            Node::Control {
                control: Control::Mode { .. },
            } => return Err(unsupported("play mode timing")),
            _ => {}
        }
    }
    Ok(groups)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Formal plays share the authority's execution and rollback boundary."
)]
impl Game {
    pub(in crate::game::internal) fn internal_quick_preflight(
        &self,
        holder: &str,
        ability: &Ability,
        actor: PlayerId,
        spell: bool,
    ) -> Result<bool> {
        // Allocate evaluation slots on a disposable authority, without moving cards or running effects.
        let mut probe = self.clone();
        let mut execution = probe.new_card_execution(Context {
            source: holder.into(),
            generation: Generation(self.object(holder)?.generation),
            controller: actor,
        })?;
        let play = ability
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing Quick PlaySpec"))?;
        execution.captures(&probe, &play.captures)?;
        let CostSpec::Ordered { steps } = &play.costs else {
            return Err(unsupported("Quick costs"));
        };
        let mut pp = int(&self.player(seat_name(actor)?)?.pp["current"]);
        let mut standing = self.object(holder)?.state["acted"] == false;
        for step in steps {
            match &step.instruction.action {
                Action::Resource {
                    amount: Expr::Literal(Int(amount)),
                    ..
                } => {
                    let amount = if spell {
                        self.internal_quote_card_cost(&execution)?
                    } else {
                        *amount
                    };
                    if pp < amount {
                        return Ok(false);
                    }
                    pp = pp
                        .checked_sub(amount)
                        .ok_or_else(|| invalid("PP quote overflow"))?;
                }
                Action::Posture { .. } => {
                    if !standing {
                        return Ok(false);
                    }
                    standing = false;
                }
                Action::Move { .. } => {}
                _ => return Err(unsupported("Quick cost proof")),
            }
        }
        let groups = collect_modes(
            ability
                .body
                .as_ref()
                .ok_or_else(|| invalid("missing Quick body"))?,
        )?;
        self.quick_mode_plan(&execution, &groups, play.parameters.clone().into())
    }

    fn quick_mode_plan(
        &self,
        execution: &Execution,
        groups: &[PlayMode],
        parameters: VecDeque<Decision>,
    ) -> Result<bool> {
        let Some((group, rest)) = groups.split_first() else {
            return self.internal_feasible_targets(execution, &execution.slots, &parameters);
        };
        let count = execution.natural(self, &group.count)?;
        let ChoiceDomain::Modes { cardinality, .. } = &group.chosen.domain else {
            return Err(unsupported("Quick mode domain"));
        };
        if count == Nat(0) {
            return self.quick_mode_plan(execution, rest, parameters);
        }
        let names = group
            .modes
            .iter()
            .enumerate()
            .map(|(i, _)| i.to_string())
            .collect::<Vec<_>>();
        let maximum = cardinality.maximum.map_or(count.0, |n| n.0.min(count.0));
        for selection in subsets(
            &names,
            usize::try_from(cardinality.minimum.0).map_err(invalid)?,
            usize::try_from(maximum).map_err(invalid)?,
        ) {
            let selected = group
                .modes
                .iter()
                .enumerate()
                .filter(|(index, _)| selection.contains(&index.to_string()))
                .map(|(_, mode)| mode.id)
                .collect::<Vec<_>>();
            if !group.chosen.accepts_answer(count, &selected) {
                continue;
            }
            let mut parameters = parameters.clone();
            for (index, mode) in group.modes.iter().enumerate() {
                if selection.contains(&index.to_string()) {
                    parameters.extend(mode.parameters.clone());
                }
            }
            if self.quick_mode_plan(execution, rest, parameters)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Play an admitted ordinary Spell, follower or amulet.
    ///
    /// # Errors
    /// Rejects unsupported cards, timing, carry, payment and body failures atomically.
    pub fn play_internal_card(&mut self, holder: &str, actor: PlayerId) -> Result<CardPlay> {
        if self.card_runtime.quick.is_some() {
            return Err(invalid("Quick requires its authority question"));
        }
        let before = self.clone();
        let result = if self.object_type(holder)? == "スペル" {
            self.play_internal_spell(holder, actor)
        } else {
            self.play_internal_field_card(holder, actor)
        };
        self.finish_card_attempt(before, result)
    }

    pub(super) fn play_internal_spell(
        &mut self,
        holder: &str,
        actor: PlayerId,
    ) -> Result<CardPlay> {
        self.check_card_play_holder(holder, actor)?;
        let object = self.object(holder)?.clone();
        let card = self
            .card_runtime
            .cards
            .get(&object.card)
            .ok_or_else(|| invalid("card is not loaded"))?;
        let abilities = card
            .card()
            .abilities
            .iter()
            .filter(|a| a.node.kind == AbilityKind::Spell)
            .collect::<Vec<_>>();
        let [located] = abilities.as_slice() else {
            return Err(unsupported("one printed Spell per card"));
        };
        let ability = located.node.clone();
        let (face, _) = printed_origin(&ability.source.origin)?;
        if face_ordinal(face)? != 0 {
            return Err(unsupported("Spell belongs to another face"));
        }
        let old_context = Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: actor,
        };
        let mut execution = self.new_card_execution(old_context)?;
        let play = ability
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing Spell play"))?;
        execution.captures(self, &play.captures)?;
        self.internal_play_move(holder, "resolution", &execution.frame())?;
        self.object_mut(holder)?.state["entered_from"] = json!(object.zone);
        execution.context.generation = Generation(self.object(holder)?.generation);
        let source = self.internal_card_source(holder, ability.id, actor)?;
        // The Spell's source is its Resolution generation, never the Hand/EX play-time object.
        execution.bind_source(self, source)?;
        execution.spell = true;
        self.run_card_play(execution, ability)
    }

    pub(super) fn check_card_play_holder(&self, holder: &str, actor: PlayerId) -> Result<()> {
        let object = self.object(holder)?;
        let seat = seat_name(actor)?;
        self.internal_context(&Context {
            source: holder.into(),
            generation: Generation(object.generation),
            controller: actor,
        })?;
        if object.owner != seat
            || object.controller != seat
            || !matches!(object.zone.as_str(), "hand" | "ex")
            || !self.internal_play_timing(actor, self.internal_quick_spell(holder)?)
            || int(&object.state["face"]) != 0
            || !self.prepared.is_empty()
            || object.state["evolved_with"].is_string()
            || self.restricted(holder, "play")?
        {
            return Err(unsupported("card play holder, permission or timing"));
        }
        self.check_internal_cards()
    }

    pub(super) fn start_internal_ability(
        &mut self,
        ability: Ability,
        context: Context,
        source: RuntimeAbilitySource,
        spell: bool,
    ) -> Result<CardPlay> {
        let mut execution = self.new_card_execution(context)?;
        execution.bind_source(self, source)?;
        if let Some(play) = &ability.play {
            execution.captures(self, &play.captures)?;
        }
        execution.spell = spell;
        self.run_card_play(execution, ability)
    }

    fn internal_play_counts(&self) -> Result<BTreeMap<u32, (Nat, Nat)>> {
        let mut counts = BTreeMap::new();
        for seat in [1, 2] {
            let cards = self.zone_ids(seat_name(PlayerId { seat })?, "cemetery");
            let mut spells = 0_u64;
            for id in &cards {
                if self.object_type(id)? == "スペル" {
                    spells = spells
                        .checked_add(1)
                        .ok_or_else(|| invalid("SpellChain count overflow"))?;
                }
            }
            counts.insert(
                seat,
                (
                    Nat(spells),
                    Nat(u64::try_from(cards.len()).map_err(invalid)?),
                ),
            );
        }
        Ok(counts)
    }

    pub(super) fn new_card_execution(&mut self, context: Context) -> Result<Execution> {
        self.emitted.clear();
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        self.state.ir_play_clock = self
            .state
            .ir_play_clock
            .checked_add(1)
            .ok_or_else(|| invalid("proper play time exhausted"))?;
        Ok(Execution {
            context,
            id: ExecutionId(self.state.ir_epoch),
            timestamp: self.state.ir_play_clock,
            ids: self.internal_identities()?,
            todo: VecDeque::new(),
            slots: BTreeMap::new(),
            receipts: BTreeMap::new(),
            facts: Vec::new(),
            waiting: None,
            destroy_guards: Vec::new(),
            look_viewers: BTreeMap::new(),
            allocations: BTreeMap::new(),
            origin: None,
            ability: None,
            source_information: None,
            entry_object: None,
            damage_event: None,
            targets: BTreeMap::new(),
            play: None,
            spell: false,
            field_entry: false,
            card_payment: None,
            play_counts: self.internal_play_counts()?,
        })
    }

    pub(super) fn run_card_play(
        &mut self,
        mut execution: Execution,
        ability: Ability,
    ) -> Result<CardPlay> {
        let body = ability.body.ok_or_else(|| invalid("missing body"))?;
        let groups = collect_modes(&body)?;
        let play = ability.play.unwrap_or(PlaySpec {
            limit: None,
            captures: Vec::new(),
            parameters: Vec::new(),
            costs: CostSpec::Ordered { steps: Vec::new() },
        });
        execution.play = Some(PlayState {
            snapshot: None,
            snapshot_id: SnapshotId(self.state.ir_epoch),
            modes: groups.clone().into(),
            groups,
            chosen: BTreeMap::new(),
            parameters: play.parameters.into(),
            exchange: if execution.spell {
                self.internal_exchange(&execution)?
            } else {
                None
            },
            costs: play.costs,
            limit: play.limit,
            body,
            restart: None,
            distribution_captures: None,
        });
        self.internal_run(&mut execution)?;
        self.card_play_result(execution)
    }

    pub(super) fn finish_card_attempt(
        &mut self,
        before: Self,
        result: Result<CardPlay>,
    ) -> Result<CardPlay> {
        if result.is_err() {
            let high = self.state.ir_epoch;
            *self = before;
            if self.card_runtime.quick.is_some() {
                self.state.ir_epoch = self.state.ir_epoch.max(high);
                self.refresh_internal_quick()?;
            }
            return result;
        }
        if let Some(execution) = self.typed.as_mut() {
            execution.origin = Some(Box::new(before));
        }
        result
    }

    pub(super) fn card_play_result(&mut self, execution: Execution) -> Result<CardPlay> {
        if let Some((view, _)) = &execution.waiting {
            let view = view.clone();
            self.typed = Some(execution);
            Ok(CardPlay::Awaiting(view))
        } else if let Some(entry) = self.card_runtime.entry_result.take() {
            Ok(CardPlay::Entered(entry))
        } else {
            activation(execution.progress(), execution.ability).map(CardPlay::Resolved)
        }
    }

    /// Answer the authority's pending formal play, without supplying execution context.
    ///
    /// # Errors
    /// Wrong answers preserve the question; commit failures restore the entire attempt.
    pub fn answer_internal_card(
        &mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: Answer,
    ) -> Result<CardPlay> {
        if self.card_runtime.quick.is_some() {
            return self.answer_internal_quick(actor, token, &answer);
        }
        if self.typed.is_none() && self.card_runtime.question.is_some() {
            return self.answer_internal_pending(actor, token, &answer);
        }
        if self
            .typed
            .as_ref()
            .is_none_or(|execution| execution.ability.is_none() && !execution.field_entry)
        {
            return Err(invalid("no pending formal play"));
        }
        let source = self.typed.as_ref().and_then(|execution| execution.ability);
        let progress = self.answer_internal_step(actor, token, answer)?;
        if let Some((view, _)) = self
            .typed
            .as_ref()
            .and_then(|execution| execution.waiting.as_ref())
        {
            Ok(CardPlay::Awaiting(view.clone()))
        } else if let Some(entry) = self.card_runtime.entry_result.take() {
            Ok(CardPlay::Entered(entry))
        } else {
            activation(progress, source).map(CardPlay::Resolved)
        }
    }

    pub(in crate::game::internal) fn internal_play_parameters(
        &mut self,
        execution: &mut Execution,
    ) -> Result<()> {
        let Some(mut play) = execution.play.take() else {
            return Ok(());
        };
        if self.internal_exchange_decision(execution, &play)?
            || self.internal_play_modes(execution, &mut play)?
        {
            execution.play = Some(play);
            return Ok(());
        }
        play.freeze_selection(self, execution)?;
        while let Some(decision) = play.parameters.pop_front() {
            let snapshot = play
                .snapshot
                .as_ref()
                .ok_or_else(|| invalid("missing selection snapshot"))?;
            match &decision.domain {
                ChoiceDomain::Objects { .. } => {
                    snapshot.internal_target(execution, &decision)?;
                    // Target construction is pure; the live authority alone allocates the token.
                    if let Some((view, _)) = execution.waiting.as_mut() {
                        self.state.ir_epoch = self
                            .state
                            .ir_epoch
                            .checked_add(1)
                            .ok_or_else(|| invalid("IR epoch exhausted"))?;
                        view.token.epoch = self.state.ir_epoch;
                    }
                }
                ChoiceDomain::Distribution {
                    recipients, total, ..
                } => {
                    let members = execution.recipients(snapshot, recipients)?;
                    let total = execution.natural(snapshot, total)?;
                    if !members.members().is_empty()
                        && u64::try_from(members.members().len()).map_err(invalid)? > total.0
                    {
                        Self::retry_play_targets(execution, &mut play)?;
                        execution.play = Some(play);
                        return self.internal_play_parameters(execution);
                    }
                    // Distribution needs the same frozen game and the authority's live token allocator.
                    self.internal_distribution_in(
                        execution,
                        &decision,
                        snapshot,
                        play.snapshot_id,
                    )?;
                    if execution.waiting.is_some() {
                        play.distribution_captures = Some(decision);
                    } else {
                        execution.captures(snapshot, &decision.captures)?;
                    }
                }
                _ => return Err(unsupported("play parameter")),
            }
            if execution.waiting.is_some() {
                execution.play = Some(play);
                return Ok(());
            }
        }
        if self.internal_exchange_materials(execution, &mut play)? {
            execution.play = Some(play);
            return Ok(());
        }
        self.finish_play_payment(execution, play)
    }

    fn finish_play_payment(&mut self, execution: &mut Execution, play: PlayState) -> Result<()> {
        let previous = self.card_runtime.active_source;
        self.card_runtime.active_source = execution.ability;
        let payment = if play.exchange.is_some() || execution.field_entry {
            self.internal_exchange_payment(execution, &play)
        } else {
            self.internal_card_payment(execution, &play.costs, execution.spell)
        };
        self.card_runtime.active_source = previous;
        payment?;
        if execution.field_entry {
            return Ok(());
        }
        self.commit_internal_play(execution, play)
    }

    fn commit_internal_play(&mut self, execution: &mut Execution, play: PlayState) -> Result<()> {
        let source = execution
            .ability
            .ok_or_else(|| invalid("missing formal source"))?;
        if execution.spell {
            self.bump(
                &format!("{}.cards_played", seat_name(source.controller)?),
                1,
            );
            let group = self.group();
            self.emit(
                json!({"kind":"プレイ","object":execution.context.source}),
                &execution.frame().cause,
                group,
            );
        }
        let turn = TurnId(self.state.schedule.current()?);
        if let Some(limit) = play.limit {
            self.card_runtime
                .consume_quota(source.instance, turn, limit)?;
        }
        for group in &play.groups {
            if let ChoiceDomain::Modes {
                exclude_used: Some(_),
                ..
            } = group.chosen.domain
            {
                for mode in play
                    .chosen
                    .get(&group.chosen.id)
                    .ok_or_else(|| invalid("missing modes"))?
                {
                    let fact = ModeUse {
                        execution: execution.id,
                        ability_instance: source.instance,
                        mode: *mode,
                        turn,
                    };
                    if !self.card_runtime.mode_uses.contains(&fact) {
                        self.card_runtime.mode_uses.push(fact);
                    }
                }
            }
        }
        execution.facts.push(if execution.spell {
            Fact::CardPlayed {
                execution: execution.id,
                object: source.holder,
                controller: source.controller,
            }
        } else {
            Fact::AbilityPlayed {
                execution: execution.id,
                source,
            }
        });
        execution.prepend(selected_body(play.body, &play.chosen)?);
        Ok(())
    }

    fn internal_play_modes(
        &mut self,
        execution: &mut Execution,
        play: &mut PlayState,
    ) -> Result<bool> {
        while let Some(group) = play.modes.pop_front() {
            let count = execution.natural(self, &group.count)?;
            let ChoiceDomain::Modes {
                modes,
                cardinality,
                exclude_used,
                ..
            } = &group.chosen.domain
            else {
                return Err(unsupported("mode domain"));
            };
            let count = Nat(count.0.min(u64::try_from(modes.len()).map_err(invalid)?));
            if count == Nat(0) {
                play.chosen.insert(group.chosen.id, Vec::new());
                continue;
            }
            let minimum = if cardinality.maximum.is_none() {
                1
            } else {
                cardinality.minimum.0
            };
            let source = execution
                .ability
                .ok_or_else(|| invalid("missing mode source"))?;
            let turn = TurnId(self.state.schedule.current()?);
            let mut legal = Vec::new();
            for mode in &group.modes {
                if self.internal_mode_targets_feasible(execution, &mode.parameters)?
                    && (exclude_used.is_none()
                        || !self.card_runtime.mode_uses.iter().any(|used| {
                            used.ability_instance == source.instance
                                && used.turn == turn
                                && used.mode == mode.id
                        }))
                {
                    legal.push(mode.id);
                }
            }
            let affordable = self.internal_play_affordable(execution, play)?;
            if u64::try_from(legal.len()).map_err(invalid)? < minimum || !affordable {
                return Err(crate::EngineFailure::IllegalPlay(
                    "no legal mode and cost plan".into(),
                ));
            }
            let mut chosen = group.chosen;
            let ChoiceDomain::Modes {
                modes: legal_modes,
                cardinality: legal_bounds,
                ..
            } = &mut chosen.domain
            else {
                return Err(unsupported("mode domain"));
            };
            *legal_modes = legal;
            *legal_bounds = Cardinality {
                minimum: Nat(minimum),
                maximum: Some(count),
            };
            self.internal_wait(
                execution,
                chosen.id,
                execution.context.controller,
                InputDomain::Modes {
                    modes: legal_modes.clone(),
                    cardinality: *legal_bounds,
                    count,
                },
                Wait::Modes(ModeWait { chosen, count }),
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    fn internal_play_affordable(&self, execution: &Execution, play: &PlayState) -> Result<bool> {
        if play.exchange.is_some() {
            return self.internal_exchange_affordable(execution, play);
        }
        let mut payment_game = self.clone();
        payment_game.card_runtime.active_source = execution.ability;
        match payment_game.internal_card_payment(
            &mut execution.clone(),
            &play.costs,
            execution.spell,
        ) {
            Ok(()) => Ok(true),
            Err(crate::EngineFailure::IllegalPlay(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(in crate::game::internal) fn internal_play_distribution_captures(
        execution: &mut Execution,
        decision: DecisionId,
    ) -> Result<()> {
        if let Some(mut play) = execution.play.take() {
            if let Some(captured) = play.distribution_captures.take() {
                if captured.id != decision {
                    return Err(invalid("foreign distribution captures"));
                }
                execution.captures(
                    play.snapshot
                        .as_ref()
                        .ok_or_else(|| invalid("missing snapshot"))?,
                    &captured.captures,
                )?;
            }
            execution.play = Some(play);
        }
        Ok(())
    }

    pub(in crate::game::internal) fn internal_target_answer(
        execution: &mut Execution,
        wait: &super::selection::TargetWait,
        objects: Vec<ObjectRef>,
    ) -> Result<()> {
        let play = execution
            .play
            .as_ref()
            .ok_or_else(|| invalid("missing target play"))?;
        let snapshot = play
            .snapshot
            .clone()
            .ok_or_else(|| invalid("missing target snapshot"))?;
        execution.bind_targets(wait, objects);
        execution.captures(&snapshot, &wait.decision.captures)
    }

    fn retry_play_targets(execution: &mut Execution, play: &mut PlayState) -> Result<()> {
        let restart = play
            .restart
            .as_ref()
            .ok_or_else(|| invalid("missing target rollback"))?;
        // Feasibility was proven against this same snapshot before the first target question.
        execution.slots.clone_from(&restart.slots);
        execution.targets.clear();
        execution.allocations.clear();
        play.parameters.clone_from(&restart.parameters);
        play.distribution_captures = None;
        Ok(())
    }

    pub(in crate::game::internal) fn internal_spell_cleanup(
        &mut self,
        execution: &mut Execution,
    ) -> Result<()> {
        if execution.spell {
            let holder = &execution.context.source;
            let current = self.object(holder)?;
            if current.zone == "resolution" && current.generation == execution.context.generation.0
            {
                current
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("Spell cleanup generation exhausted"))?;
                let departing_source = execution.source_before_move(self, from_ref(holder))?;
                self.move_objects(from_ref(holder), "cemetery", None, None, &execution.frame())?;
                execution.seal_source_departure(departing_source);
            }
        }
        Ok(())
    }
}

fn activation(
    progress: super::super::Progress,
    source: Option<RuntimeAbilitySource>,
) -> Result<Activation> {
    let source = source.ok_or_else(|| invalid("missing formal source"))?;
    let execution = match progress.facts.last() {
        Some(Fact::AbilityResolved {
            execution,
            source: resolved,
        }) if *resolved == source => *execution,
        Some(Fact::CardResolved {
            execution,
            object,
            controller,
        }) if *object == source.holder && *controller == source.controller => *execution,
        _ => return Err(invalid("formal play did not complete")),
    };
    Ok(Activation {
        execution,
        source,
        progress,
        resume: ResumeStage::CheckTiming,
    })
}

fn selected_body(
    mut program: Program,
    choices: &BTreeMap<DecisionId, Vec<ModeId>>,
) -> Result<Program> {
    let mut steps = Vec::new();
    for mut located in program.steps {
        match located.node {
            Node::Control {
                control: Control::Mode { modes, chosen, .. },
            } => {
                let chosen = choices
                    .get(&chosen.id)
                    .ok_or_else(|| invalid("missing saved mode result"))?;
                for mode in modes {
                    if chosen.contains(&mode.id) {
                        steps.extend(mode.body.steps);
                    }
                }
            }
            Node::Control {
                control: Control::Sequence { body },
            } => {
                located.node = Node::Control {
                    control: Control::Sequence {
                        body: selected_body(body, choices)?,
                    },
                };
                steps.push(located);
            }
            _ => steps.push(located),
        }
    }
    program.steps = steps;
    Ok(program)
}

fn independent_selector(selector: &Selector) -> bool {
    match selector {
        Selector::Zone { .. } | Selector::Objects { .. } => true,
        Selector::Union { sets } => sets.iter().all(independent_selector),
        _ => false,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Field plays share the card payment continuation."
)]
impl Game {
    pub(super) fn run_field_play(&mut self, mut execution: Execution) -> Result<CardPlay> {
        execution.field_entry = true;
        execution.play = Some(PlayState {
            snapshot: None,
            snapshot_id: SnapshotId(self.state.ir_epoch),
            modes: VecDeque::new(),
            groups: vec![],
            chosen: BTreeMap::new(),
            parameters: VecDeque::new(),
            exchange: self.internal_exchange(&execution)?,
            costs: CostSpec::Ordered { steps: vec![] },
            limit: None,
            body: Program {
                scope: ScopeId(0),
                steps: vec![],
            },
            restart: None,
            distribution_captures: None,
        });
        self.internal_run(&mut execution)?;
        self.card_play_result(execution)
    }
}
