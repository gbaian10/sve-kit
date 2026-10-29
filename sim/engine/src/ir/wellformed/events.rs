use crate::ir::events::played::PlayedFilter;
use crate::ir::history_turn::played_by;
use crate::ir::{DamageRecipient, PlayEvent};
use crate::ir::{ZoneFrozen, ZoneKind, ZoneOperation, ZoneRef};
macro_rules! projected_equality {
    ($checker:expr, $equality:expr; $($variant:ident),+ $(,)?) => {
        match $equality {
            $(ValueEquality::$variant { left, right } => {
                $checker.projection(left)?;
                $checker.projection(right)
            },)+
            ValueEquality::Bool { left, right } => {
                $checker.boolean(left)?;
                $checker.boolean(right)
            }
            ValueEquality::InfoInt { .. } | ValueEquality::InfoCollaborationName { .. } | ValueEquality::InfoPaymentReceipt { .. } | ValueEquality::TurnId { .. } | ValueEquality::VecMoveResult { .. } => $checker.reject(),
        }
    };
}

use super::{BindingKind, Checked, Checker, WellFormedReason};
use crate::ir::{
    AssociationIdFields, CauseHeaderFields, CharacteristicsFields, CheckIdFields, CommitIdFields,
    CompletionFields, CounterKindFields, DamageFlagsFields, DecisionIdFields, EdgeFields,
    EntryMethodFields, EntryOriginFields, EventIdHeaderFields, EvolutionStateFields,
    ExecutionIdFields, FacetFields, GameTimeHeaderFields, HolderFields, InstructionIdFields,
    KindFrozen, MoveMethodFields, NamesFrozen, NumericHistoryQuery, OptionBatchIdHeaderFields,
    OptionExecutionIdHeaderFields, OptionInstructionIdFields,
    OptionRuntimeAbilitySourceHeaderFields, OptionRuntimeReceiptIdHeaderFields,
    OptionZoneKindFields, OriginFactIdFields, PaidComponentFields, PaymentResultFields,
    PaymentTimeFields, PhaseFields, PlaySubjectFields, PositionFields, PostureFields,
    ReceiptMovesRead, RecipientFields, RollIdFields, RuntimeAbilityReferenceFields,
    RuntimeAbilitySourceFields, RuntimeReceiptIdFields, RuntimeVisibilityHeaderFields,
    SnapshotFields, SnapshotIdHeaderFields, StatChangeReasonFields, TimestampHeaderFields,
    TraitsFrozen, TriggerIconFields, TurnIdFields, ValueEquality, VecDieFaceFields,
    VecRuntimeObjectViewFields, VecRuntimeReceiptIdFields,
};
use crate::ir::{
    Binding, Bool, CausePattern, EffectSource, EventContext, EventPattern, EventSnapshotPolicy,
    ExecutionRef, Expr, HistoryAnchor, HistoryCutoff, HistoryQuery, HistoryRange, HistoryWindow,
    InclusionPolicy, LkiReason, MovementEvent, ObjectFrozen, ObjectOperand, ObjectRef, ObjectView,
    PlayerFrozen, PlayerRead, PlayerRef, Predicate, Role, Value,
};

// Each typed payload binding denotes a payload together with its original envelope.
pub(super) trait Projected {
    fn check(&self, checker: &Checker) -> Checked;
}

macro_rules! payload_fields {
    ($ty:ty; $($variant:ident),+ $(,)?) => {
        impl Projected for $ty {
            fn check(&self, checker: &Checker) -> Checked {
                match self {
                    $(Self::$variant { event } => checker.reference(*event, BindingKind::Value),)+
                }
            }
        }
    };
}
macro_rules! header_fields {
    ($ty:ty; $($variant:ident),+ $(,)?) => {
        impl Projected for $ty {
            fn check(&self, checker: &Checker) -> Checked {
                match self {
                    $(Self::$variant { event } => checker.event_context(*event),)+
                }
            }
        }
    };
}

macro_rules! snapshot_fields {
    ($ty:ty) => {
        impl Projected for $ty {
            fn check(&self, checker: &Checker) -> Checked {
                match self {
                    Self::Snapshot { object } => checker.projection(object),
                    Self::Characters { characters } => checker.projection(characters),
                }
            }
        }
    };
}
snapshot_fields!(NamesFrozen);
snapshot_fields!(KindFrozen);
snapshot_fields!(TraitsFrozen);

payload_fields!(PositionFields; MovePosition);
payload_fields!(MoveMethodFields; MoveMethod);
payload_fields!(OptionZoneKindFields; MoveEnteredFrom);
payload_fields!(SnapshotFields; DamageSource, LeaveFieldLki);
payload_fields!(RecipientFields; DamageRecipient, AttackTarget);
payload_fields!(DamageFlagsFields; DamageFlags);
payload_fields!(RollIdFields; DiceRoll);
payload_fields!(VecDieFaceFields; DiceAttempts);
payload_fields!(EntryOriginFields; EnterFieldFrom, EnterExFrom);
payload_fields!(CharacteristicsFields; EnterFieldCharacters, EnterExCharacters, DiscardCharacters, DestroyCharacters, BanishCharacters, EvolveBefore, EvolveAfter);
payload_fields!(EntryMethodFields; EnterFieldMethod);
payload_fields!(PlaySubjectFields; PlaySubject);
payload_fields!(RuntimeReceiptIdFields; PlayCost, DriveTriggerEffectReceipt, FusedPayment, EquipCreatedReceipt, DriveCheckedReceipt);
payload_fields!(ExecutionIdFields; TargetedBy, ResolveExecution, UbActivatedExecution, FusedExecution);
payload_fields!(DecisionIdFields; TargetedSelection);
payload_fields!(RuntimeAbilityReferenceFields; ResolveAbility, UbActivatedAbility);
payload_fields!(CompletionFields; ResolveCompletion);
payload_fields!(VecRuntimeReceiptIdFields; ResolveReceipts);
payload_fields!(PaymentResultFields; PaymentPayment);
payload_fields!(PaidComponentFields; PaymentComponent);
payload_fields!(PaymentTimeFields; PaymentPhase);
payload_fields!(InstructionIdFields; DrawInstruction);
payload_fields!(HolderFields; StatChangeSubject, CounterChangeHolder);
payload_fields!(FacetFields; StatChangeFacets);
payload_fields!(StatChangeReasonFields; StatChangeReason);
payload_fields!(CommitIdFields; StatChangeCommit);
payload_fields!(OptionInstructionIdFields; StatChangeInstruction);
payload_fields!(CounterKindFields; CounterChangeKind);
payload_fields!(PostureFields; PostureBefore, PostureAfter);
payload_fields!(EvolutionStateFields; EvolveKind);

impl Projected for TurnIdFields {
    fn check(&self, checker: &Checker) -> Checked {
        match self {
            Self::HeaderTurn { event } => checker.event_context(*event),
            Self::PhaseTurn { event } => checker.reference(*event, BindingKind::Value),
        }
    }
}

impl Projected for PhaseFields {
    fn check(&self, checker: &Checker) -> Checked {
        match self {
            Self::HeaderPhase { event } => checker.event_context(*event),
            Self::PhasePhase { event } => checker.reference(*event, BindingKind::Value),
        }
    }
}
payload_fields!(EdgeFields; PhaseEdge);
payload_fields!(AssociationIdFields; RaceLink, EquipAssociation);
payload_fields!(TriggerIconFields; DriveTriggerIcon);
payload_fields!(CheckIdFields; DriveTriggerCheck, DriveCheckedCheck);

impl Projected for OriginFactIdFields {
    fn check(&self, checker: &Checker) -> Checked {
        match self {
            Self::HeaderOrigin { event } => checker.event_context(*event),
            Self::UbActivatedPlayFact { event } => checker.reference(*event, BindingKind::Value),
        }
    }
}
payload_fields!(VecRuntimeObjectViewFields; FusedMaterials);
payload_fields!(RuntimeAbilitySourceFields; EquipSource, DriveCheckedHolder);
header_fields!(EventIdHeaderFields; HeaderId);
header_fields!(OptionBatchIdHeaderFields; HeaderBatch);
header_fields!(OptionExecutionIdHeaderFields; HeaderExecution);
header_fields!(CauseHeaderFields; HeaderCause);
header_fields!(OptionRuntimeAbilitySourceHeaderFields; HeaderSource);
header_fields!(GameTimeHeaderFields; HeaderTime);
header_fields!(SnapshotIdHeaderFields; HeaderBefore, HeaderAfter);
header_fields!(OptionRuntimeReceiptIdHeaderFields; HeaderReceipt);
header_fields!(RuntimeVisibilityHeaderFields; HeaderVisibility);
header_fields!(TimestampHeaderFields; HeaderPoint);

impl Projected for ObjectFrozen {
    fn check(&self, checker: &Checker) -> Checked {
        match self {
            Self::SourceHolder {} => Ok(()),
            Self::SnapshotObject { snapshot } => checker.projection(snapshot),
            Self::EnterFieldObject { event } => checker.reference(*event, BindingKind::Value),
            Self::TargetedObject { event } => checker.reference(*event, BindingKind::Value),
            Self::LeaveFieldObjectBefore { event } => checker.reference(*event, BindingKind::Value),
            Self::EnterExObject { event } => checker.reference(*event, BindingKind::Value),
            Self::DrawObject { event } => checker.reference(*event, BindingKind::Value),
            Self::DiscardCardBefore { event } => checker.reference(*event, BindingKind::Value),
            Self::DestroyObjectBefore { event } => checker.reference(*event, BindingKind::Value),
            Self::BanishCardBefore { event } => checker.reference(*event, BindingKind::Value),
            Self::PostureObject { event } => checker.reference(*event, BindingKind::Value),
            Self::EvolveSubject { event } | Self::EvolveWith { event } => {
                checker.reference(*event, BindingKind::Value)
            }
            Self::AttackAttacker { event } => checker.reference(*event, BindingKind::Value),
            Self::RaceSubject { event } => checker.reference(*event, BindingKind::Value),
            Self::DriveGainedSubject { event } => checker.reference(*event, BindingKind::Value),
            Self::DriveTriggerCheckedCard { event } => {
                checker.reference(*event, BindingKind::Value)
            }
            Self::UbActivatedHolder { event } => checker.reference(*event, BindingKind::Value),
            Self::FusedSubjectBefore { event } | Self::FusedSubjectAfter { event } => {
                checker.reference(*event, BindingKind::Value)
            }
            Self::EquipHolder { event } | Self::EquipEquipment { event } => {
                checker.reference(*event, BindingKind::Value)
            }
            Self::DriveCheckedCheckedCard { event } => {
                checker.reference(*event, BindingKind::Value)
            }
            Self::MoveBefore { event } | Self::MoveAfter { event } => {
                checker.reference(*event, BindingKind::Value)
            }
        }
    }
}
impl Projected for PlayerFrozen {
    fn check(&self, checker: &Checker) -> Checked {
        match self {
            Self::HeaderActor { event } => checker.event_context(*event),
            Self::DiceRoller { event } => checker.reference(*event, BindingKind::Value),
            Self::EnterFieldController { event } => checker.reference(*event, BindingKind::Value),
            Self::PlayController { event } => checker.reference(*event, BindingKind::Value),
            Self::TargetedChooser { event } => checker.reference(*event, BindingKind::Value),
            Self::LeaveFieldController { event } => checker.reference(*event, BindingKind::Value),
            Self::EnterExController { event } => checker.reference(*event, BindingKind::Value),
            Self::DrawPlayer { event } => checker.reference(*event, BindingKind::Value),
            Self::DiscardPlayer { event } => checker.reference(*event, BindingKind::Value),
            Self::DestroyController { event } => checker.reference(*event, BindingKind::Value),
            Self::BanishActor { event } => checker.reference(*event, BindingKind::Value),
            Self::LifeChangeSubject { event } => checker.reference(*event, BindingKind::Value),
            Self::AttackController { event } => checker.reference(*event, BindingKind::Value),
            Self::PhasePlayer { event } => checker.reference(*event, BindingKind::Value),
            Self::RaceController { event } => checker.reference(*event, BindingKind::Value),
            Self::DriveGainedController { event } => checker.reference(*event, BindingKind::Value),
            Self::DriveTriggerPlayer { event } => checker.reference(*event, BindingKind::Value),
            Self::UbActivatedController { event } => checker.reference(*event, BindingKind::Value),
            Self::DriveCheckedPlayer { event } => checker.reference(*event, BindingKind::Value),
            Self::DamageController { event } => checker.reference(*event, BindingKind::Value),
        }
    }
}

impl Projected for ReceiptMovesRead {
    fn check(&self, checker: &Checker) -> Checked {
        let Self::Moves { receipt } = self;
        checker.movement_receipt(receipt)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Event scopes and projections share the structural checker."
)]
impl Checker {
    pub(super) fn event_field<T>(&self, binding: Binding<T>) -> Checked
    where
        T: 'static,
    {
        self.reference(binding, BindingKind::Value)?;
        if self.env.chooser.is_some() {
            return self.reject();
        }
        Ok(())
    }
    pub(super) fn projection<T>(&self, expression: &Expr<T>) -> Checked
    where
        T: Value + 'static,
        T::Frozen: Projected,
    {
        match expression {
            Expr::Literal(_) => Ok(()),
            Expr::Bound(binding) => {
                self.reference(*binding, BindingKind::Value)?;
                self.visible(self.env.access.get(&binding.key()))
            }
            Expr::Captured(reference) => {
                self.reference(reference.binding, BindingKind::Capture)?;
                self.visible(self.env.access.get(&reference.binding.key()))
            }
            Expr::Frozen(field) if self.env.chooser.is_none() => field.check(self),
            Expr::Frozen(_) | Expr::Live(_) | Expr::Compute(_) => self.reject(),
        }
    }

    pub(super) fn event_context(&self, context: EventContext) -> Checked {
        match context {
            EventContext::Trigger {} if self.env.trigger.is_some() => Ok(()),
            EventContext::Trigger {} => self.reject(),
            EventContext::Bound { binding } => self.reference(binding, BindingKind::Value),
            EventContext::Move { event } => self.reference(event, BindingKind::Value),
            EventContext::Damage { event } => self.reference(event, BindingKind::Value),
            EventContext::Dice { event } => self.reference(event, BindingKind::Value),
            EventContext::EnterField { event } => self.reference(event, BindingKind::Value),
            EventContext::Play { event } => self.reference(event, BindingKind::Value),
            EventContext::Targeted { event } => self.reference(event, BindingKind::Value),
            EventContext::Resolve { event } => self.reference(event, BindingKind::Value),
            EventContext::Payment { event } => self.reference(event, BindingKind::Value),
            EventContext::LeaveField { event } => self.reference(event, BindingKind::Value),
            EventContext::EnterEx { event } => self.reference(event, BindingKind::Value),
            EventContext::Draw { event } => self.reference(event, BindingKind::Value),
            EventContext::Discard { event } => self.reference(event, BindingKind::Value),
            EventContext::Destroy { event } => self.reference(event, BindingKind::Value),
            EventContext::Banish { event } => self.reference(event, BindingKind::Value),
            EventContext::StatChange { event } => self.reference(event, BindingKind::Value),
            EventContext::LifeChange { event } => self.reference(event, BindingKind::Value),
            EventContext::CounterChange { event } => self.reference(event, BindingKind::Value),
            EventContext::Posture { event } => self.reference(event, BindingKind::Value),
            EventContext::Evolve { event } => self.reference(event, BindingKind::Value),
            EventContext::Attack { event } => self.reference(event, BindingKind::Value),
            EventContext::Phase { event } => self.reference(event, BindingKind::Value),
            EventContext::Race { event } => self.reference(event, BindingKind::Value),
            EventContext::DriveGained { event } => self.reference(event, BindingKind::Value),
            EventContext::DriveTrigger { event } => self.reference(event, BindingKind::Value),
            EventContext::UbActivated { event } => self.reference(event, BindingKind::Value),
            EventContext::Fused { event } => self.reference(event, BindingKind::Value),
            EventContext::Equip { event } => self.reference(event, BindingKind::Value),
            EventContext::DriveChecked { event } => self.reference(event, BindingKind::Value),
        }
    }

    pub(super) fn follow_event(&self, event: MovementEvent) -> Checked {
        let key = match event {
            MovementEvent::Move { event } => {
                self.reference(event, BindingKind::Value)?;
                event.key()
            }
            MovementEvent::LeaveField { event } => {
                self.reference(event, BindingKind::Value)?;
                event.key()
            }
        };
        if self.env.trigger_events.contains(&key) && self.env.chooser.is_none() {
            Ok(())
        } else {
            self.reject()
        }
    }

    pub(super) fn local_value<T>(&mut self, binding: Binding<T>, predicate: &Expr<Bool>) -> Checked
    where
        T: 'static,
    {
        self.isolated(|checker| {
            let scope = binding.key().0;
            if scope != checker.env.scope && !checker.scopes.insert(scope) {
                return Err(checker.error(WellFormedReason::DuplicateScope));
            }
            checker.env.scope = scope;
            checker.define(binding, BindingKind::Value)?;
            checker.boolean(predicate)
        })
    }

    fn played_pattern(
        &mut self,
        binding: Binding<PlayEvent>,
        predicate: &Expr<Bool>,
        view: Option<EventSnapshotPolicy>,
        triggering: bool,
    ) -> Checked {
        if view != Some(EventSnapshotPolicy::AtReveal) {
            return self.reject();
        }
        let Some(plan) = PlayedFilter::parse(binding, predicate) else {
            return self.reject();
        };
        self.define(binding, BindingKind::Value)?;
        if triggering {
            self.env.trigger_events.insert(binding.key());
        }
        self.local_value(plan.card, &Expr::Literal(Bool(true)))
    }

    pub(super) fn event_pattern(
        &mut self,
        pattern: &EventPattern,
        view: Option<EventSnapshotPolicy>,
        triggering: bool,
    ) -> Checked {
        macro_rules! event {
            ($binding:expr, $predicate:expr, $policy:expr) => {{
                if view.is_some_and(|view| view != $policy) {
                    return Err(self.error(WellFormedReason::InvalidEntry));
                }
                self.define(*$binding, BindingKind::Value)?;
                if triggering {
                    self.env.trigger_events.insert($binding.key());
                }
                self.at("predicate", |checker| checker.boolean($predicate))
            }};
        }
        match pattern {
            EventPattern::AnyOf { patterns } => {
                if patterns.is_empty() {
                    return self.reject();
                }
                for (index, alternative) in patterns.iter().enumerate() {
                    self.at(&format!("patterns/{index}"), |checker| {
                        checker.isolated(|checker| {
                            checker.event_pattern(alternative, view, triggering)
                        })
                    })?;
                }
                Ok(())
            }
            EventPattern::EarthRitePayment {
                event,
                component,
                predicate,
            } => {
                if view.is_some_and(|view| view != EventSnapshotPolicy::BeforeExit) {
                    return Err(self.error(WellFormedReason::InvalidEntry));
                }
                self.define(*event, BindingKind::Value)?;
                self.local_value(*component, predicate)
            }
            EventPattern::Payment { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::Move { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::EnterField { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::LeaveField { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::EnterEx { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::Discard { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::Destroy { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::Damage { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::StatChange { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::LifeChange { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::CounterChange { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::Evolve { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::Attack { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::Phase { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::AfterEntry)
            }
            EventPattern::Fused { binding, predicate } => {
                event!(binding, predicate, EventSnapshotPolicy::BeforeExit)
            }
            EventPattern::Play { binding, predicate } => {
                self.played_pattern(*binding, predicate, view, triggering)
            }
            EventPattern::Targeted { .. }
            | EventPattern::Resolve { .. }
            | EventPattern::Draw { .. }
            | EventPattern::Banish { .. }
            | EventPattern::Posture { .. }
            | EventPattern::Race { .. }
            | EventPattern::DriveGained { .. }
            | EventPattern::DriveTrigger { .. }
            | EventPattern::UbActivated { .. }
            | EventPattern::Equip { .. }
            | EventPattern::Dice { .. }
            | EventPattern::DriveChecked { .. } => self.reject(),
        }
    }

    pub(super) fn cause_pattern(&mut self, pattern: &CausePattern) -> Checked {
        match pattern {
            CausePattern::Effect {
                source: EffectSource::Matching { predicate },
            } => self.candidate(predicate.binding, &predicate.condition),
            CausePattern::Effect {
                source: EffectSource::Any {},
            }
            | CausePattern::Any {}
            | CausePattern::AnyRule {}
            | CausePattern::Rule { .. }
            | CausePattern::PlayerAction {} => Ok(()),
        }
    }

    pub(super) fn history_window(
        &mut self,
        window: &HistoryWindow,
        inclusion: InclusionPolicy,
    ) -> Checked {
        if self.env.chooser.is_some() {
            return self.reject();
        }
        if let HistoryRange::Since {
            anchor: HistoryAnchor::Bound { binding },
        } = window.range
        {
            self.reference(binding, BindingKind::Value)?;
        }
        if let HistoryCutoff::Capture { snapshot } = window.cutoff {
            self.reference(snapshot, BindingKind::Capture)?;
        }
        if let Some(player) = &window.relative_player {
            self.player(player)?;
        }
        if let InclusionPolicy::ExcludeCurrentExecution {
            execution: ExecutionRef::Bound { binding },
        } = inclusion
        {
            self.reference(binding, BindingKind::Value)?;
        }
        Ok(())
    }

    pub(super) fn history(&mut self, query: &HistoryQuery) -> Checked {
        if played_by(query).is_some()
            && let EventPattern::Play { binding, predicate } = &query.pattern
        {
            return self.local_value(*binding, predicate);
        }
        self.history_window(&query.window, query.include_execution)?;
        self.isolated(|checker| checker.event_pattern(&query.pattern, None, false))
    }

    pub(super) fn numeric_history(&mut self, query: &NumericHistoryQuery) -> Checked {
        macro_rules! numeric {
            ($event:expr, $predicate:expr, $window:expr, $inclusion:expr) => {{
                self.history_window($window, *$inclusion)?;
                self.local_value(*$event, $predicate)
            }};
        }
        match query {
            NumericHistoryQuery::Damage {
                event,
                predicate,
                window,
                include_execution,
            } => numeric!(event, predicate, window, include_execution),
            NumericHistoryQuery::LifeDelta {
                event,
                predicate,
                window,
                include_execution,
            } => numeric!(event, predicate, window, include_execution),
            NumericHistoryQuery::CounterDelta {
                event,
                predicate,
                window,
                include_execution,
            } => numeric!(event, predicate, window, include_execution),
            NumericHistoryQuery::StatDelta {
                event,
                predicate,
                window,
                include_execution,
                ..
            } => numeric!(event, predicate, window, include_execution),
            NumericHistoryQuery::EarthRiteRemovedStack {
                event,
                component,
                predicate,
                window,
                include_execution,
            } => {
                self.history_window(window, *include_execution)?;
                self.isolated(|checker| {
                    checker.define(*event, BindingKind::Value)?;
                    checker.local_value(*component, predicate)
                })
            }
        }
    }

    pub(super) fn player_value(&mut self, expression: &Expr<PlayerRef>) -> Checked {
        match expression {
            Expr::Literal(player) => self.player(player),
            Expr::Live(read) if !self.event_capture() => match read.as_ref() {
                PlayerRead::Active {} => Ok(()),
                PlayerRead::Role {
                    object:
                        ObjectView::LastKnown {
                            object,
                            reason: LkiReason::ZoneExit,
                        },
                    role: Role::Controller,
                } if matches!(object.as_ref(), ObjectOperand::Bound { binding } if self.env.destroyed_before.contains(&binding.key())) => {
                    self.object(object)
                }
                PlayerRead::Role { object, .. } => self.view(object),
            },
            Expr::Frozen(_)
            | Expr::Live(_)
            | Expr::Captured(_)
            | Expr::Bound(_)
            | Expr::Compute(_) => self.projection(expression),
        }
    }

    fn leave_destination(&mut self, left: &Expr<ZoneRef>, right: &Expr<ZoneRef>) -> Checked {
        let Expr::Frozen(field) = left else {
            return self.reject();
        };
        let ZoneFrozen::LeaveFieldTo { event } = field.as_ref() else {
            return self.reject();
        };
        self.reference(*event, BindingKind::Value)?;
        let Expr::Compute(operation) = right else {
            return self.reject();
        };
        let ZoneOperation::OfPlayer {
            player,
            kind: ZoneKind::Cemetery,
        } = operation.as_ref()
        else {
            return self.reject();
        };
        self.player(player)
    }

    pub(super) fn event_predicate(&mut self, predicate: &Predicate) -> Checked {
        match predicate {
            Predicate::EventCause { event, pattern } => {
                self.event_context(*event)?;
                if self.env.chooser.is_some() {
                    return self.reject();
                }
                self.cause_pattern(pattern)
            }
            Predicate::ObjectEquals { left, right } => {
                self.object_value(left)?;
                self.object_value(right)
            }
            Predicate::ZoneEquals { left, right } => self.leave_destination(left, right),
            Predicate::PhaseEquals { left, right } => {
                self.projection(left)?;
                self.projection(right)
            }
            Predicate::PlayerEquals { left, right } => {
                self.player_value(left)?;
                self.player_value(right)
            }
            Predicate::PlayerIn { player, players } => {
                self.player_value(player)?;
                self.players(players)
            }
            Predicate::AnyStatFacet {
                facets,
                facet,
                predicate,
                ..
            } => {
                self.projection(facets)?;
                self.local_value(*facet, predicate)
            }
            Predicate::AnySnapshot {
                snapshots,
                object,
                predicate,
            } => {
                self.projection(snapshots)?;
                self.local_value(*object, predicate)
            }
            Predicate::AnyMove {
                moves,
                movement,
                predicate,
            } => {
                self.projection(moves)?;
                self.local_value(*movement, predicate)
            }
            Predicate::MoveSucceeded { movement } => self.reference(*movement, BindingKind::Value),
            Predicate::RecipientIsFollower {
                recipient: DamageRecipient::FromExpression { value },
                objects,
            } => {
                self.projection(value)?;
                self.selector(objects)
            }
            Predicate::EvolutionEquals { left, right } => {
                self.projection(left)?;
                self.projection(right)
            }
            Predicate::Equal { operands } => self.equality(operands),
            Predicate::SearchFoundAny { .. }
            | Predicate::ReceiptStatusEquals { .. }
            | Predicate::PlayedCard { .. }
            | Predicate::PlayedAbility { .. }
            | Predicate::HolderIsPlayer { .. }
            | Predicate::HolderIsObject { .. }
            | Predicate::ReceiptAny { .. }
            | Predicate::RecipientIsLeader { .. }
            | Predicate::RecipientIsFollower { .. }
            | Predicate::PostureEquals { .. }
            | Predicate::TitleEquals { .. }
            | Predicate::HasAbility { .. }
            | Predicate::HasTrait { .. }
            | Predicate::NamesIntersect { .. }
            | Predicate::KindEquals { .. }
            | Predicate::ClassEquals { .. }
            | Predicate::And { .. }
            | Predicate::Or { .. }
            | Predicate::Not { .. }
            | Predicate::CompareInt { .. }
            | Predicate::CompareNat { .. }
            | Predicate::HasInfo { .. }
            | Predicate::NameEquals { .. }
            | Predicate::NameContains { .. }
            | Predicate::MemberOf { .. }
            | Predicate::Exists { .. }
            | Predicate::All { .. }
            | Predicate::Any { .. } => self.reject(),
        }
    }

    fn object_value(&self, value: &Expr<ObjectRef>) -> Checked {
        match value {
            // Identity keeps the candidate and flip-revocation rules of ObjectOperand::Bound.
            Expr::Bound(binding) => self.object(&ObjectOperand::Bound { binding: *binding }),
            Expr::Literal(_)
            | Expr::Frozen(_)
            | Expr::Live(_)
            | Expr::Captured(_)
            | Expr::Compute(_) => self.projection(value),
        }
    }

    fn equality(&mut self, equality: &ValueEquality) -> Checked {
        projected_equality!(self, equality; Timestamp, RuntimeVisibility, OptionRuntimeReceiptId, SnapshotId, GameTime, OptionRuntimeAbilitySource, Cause, OptionExecutionId, OptionBatchId, EventId, AssociationId, Characteristics, CheckId, CommitId, Completion, CounterKind, DamageFlags, DamageRecipient, DecisionId, Edge, EntryMethod, EntryOrigin, ExecutionId, Holder, InstructionId, MoveMethod, NonEmptyStatDelta, OptionInstructionId, OptionZoneKind, OriginFactId, PaidComponent, PaymentResult, PaymentTime, PlaySubject, Position, RollId, RuntimeAbilityReference, RuntimeAbilitySource, RuntimeObjectView, RuntimeReceiptId, StatChangeReason, TriggerIcon, VecDieFace, VecRuntimeObjectView, VecRuntimeReceiptId)
    }
}
