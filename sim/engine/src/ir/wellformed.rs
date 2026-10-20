//! Structural validation of a closed program body, independent of execution support.

use super::{
    AbilityOrigin, AbilitySource, Action, Arithmetic, BatchOutcome, Binding, Bool, BoolRead,
    Capture, CaptureTime, CardQuantityOutcome, Cardinality, Control, CounterOperation,
    CounterQuantityOutcome, Declaration, DiceOperation, Expr, FlipOutcome, Holder, Instruction,
    InstructionBatch, InstructionContext, InstructionId, Int, IntFrozen, IntOperation, IntRead,
    IterationCheck, Located, LookOutcome, MeasuredReceipt, MeasuredReceiptList, Nat, NatFrozen,
    NatOperation, NatRead, Node, ObjectOperand, ObjectReceipt, ObjectReceiptList, ObjectRef,
    ObjectView, PlayerRef, PlayerRelation, PlayerSet, Predicate, Program, ReceiptExport, ReceiptId,
    ReceiptList, ReceiptLocation, ReceiptReference, RepeatBody, ResourceOperation, ScopeId,
    SelectionView, Selector, SourceSpan, StepwiseMode, Termination,
};
use alloc::collections::{BTreeMap, BTreeSet};
use core::any::TypeId;
use core::error::Error;
use core::fmt;

mod control;
mod decisions;
mod entries;
mod events;
mod evolution;
mod installation;
mod modes;
mod movement;
mod producers;
mod values;
mod visibility;
pub use entries::{WellFormedCardProgram, WellFormedProcedure};

/// An owned program body whose supported shapes passed structural validation.
///
/// This is not capability admission, a checked ability, or permission to execute.
/// Neither mutable access nor a constructor bypassing validation is exposed.
/// ```compile_fail
/// use sve_engine::ir::{Program, ScopeId, WellFormedProgram};
/// let checked = WellFormedProgram { program: Program { scope: ScopeId(1), steps: vec![] } };
/// ```
/// ```compile_fail
/// use sve_engine::ir::WellFormedProgram;
/// fn mutate(checked: &mut WellFormedProgram) { checked.program().steps.clear(); }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WellFormedProgram {
    program: Program,
}
impl WellFormedProgram {
    /// Validate a closed body with no externally supplied bindings or receipts.
    ///
    /// # Errors
    /// Returns the first structural error, including shapes not yet checked.
    pub fn check(program: Program) -> Result<Self, WellFormedError> {
        let mut checker = Checker::new(program.scope);
        checker.program(&program)?;
        Ok(Self { program })
    }

    /// Inspect the exact immutable body that passed validation.
    #[must_use]
    pub const fn program(&self) -> &Program {
        &self.program
    }
}

/// Machine-readable structural failure, separate from capability availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WellFormedReason {
    /// A shape is rejected until its complete validation rules are implemented.
    UnsupportedShape,
    /// A lexical scope was declared twice.
    DuplicateScope,
    /// An instruction identity was declared twice, even on disjoint paths.
    DuplicateInstruction,
    /// A decision point was declared twice.
    DuplicateDecision,
    /// A decision is not in a supported input region.
    DecisionTiming,
    /// A payment step is not marked as a cost.
    ExpectedCost,
    /// A discard domain is not restricted to the payer's hand.
    InvalidDiscardDomain,
    /// An answer has inconsistent selection cardinalities.
    DecisionCardinality,
    /// A receipt identity was declared twice.
    DuplicateReceipt,
    /// A binding slot was declared twice, regardless of its Rust type.
    DuplicateBinding,
    /// A declaration names a different lexical scope.
    WrongScope,
    /// A binding does not dominate this use or is outside its local region.
    UnavailableBinding,
    /// A slot is referenced using a different type or capture/iteration category.
    BindingType,
    /// A result does not dominate this use.
    UnavailableReceipt,
    /// A typed receipt reference names a producer of another outcome type.
    ReceiptType,
    /// A repeat export is not a local result of its own body.
    InvalidExport,
    /// A prerequisite does not guard this instruction at its start.
    PreconditionTiming,
    /// A cost instruction is outside a payment transaction.
    CostOutsidePayment,
    /// A bounded selection has a minimum larger than its maximum.
    InvalidCardinality,
    /// An arithmetic operation has the wrong operand count.
    ArithmeticArity,
    /// A division has a statically invalid divisor.
    InvalidDivisor,
    /// A batch must contain at least one member.
    EmptyBatch,
    /// An ability or procedure has inconsistent entry metadata.
    InvalidEntry,
    /// A capture is not owned by the named entry or decision.
    CaptureTiming,
    /// Target roles cannot partition the selected set.
    InvalidRoles,
    /// A mode decision does not describe its printed modes and count.
    InvalidModes,
    /// An allocation does not belong to the damage instruction's recipient domain.
    DistributionRecipients,
    /// A movement has inconsistent destination, position, posture, or reason.
    InvalidMovement,
}

impl WellFormedReason {
    const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedShape => "UnsupportedShape",
            Self::DuplicateScope => "DuplicateScope",
            Self::DuplicateInstruction => "DuplicateInstruction",
            Self::DuplicateDecision => "DuplicateDecision",
            Self::DecisionTiming => "DecisionTiming",
            Self::ExpectedCost => "ExpectedCost",
            Self::InvalidDiscardDomain => "InvalidDiscardDomain",
            Self::DecisionCardinality => "DecisionCardinality",
            Self::DuplicateReceipt => "DuplicateReceipt",
            Self::DuplicateBinding => "DuplicateBinding",
            Self::WrongScope => "WrongScope",
            Self::UnavailableBinding => "UnavailableBinding",
            Self::BindingType => "BindingType",
            Self::UnavailableReceipt => "UnavailableReceipt",
            Self::ReceiptType => "ReceiptType",
            Self::InvalidExport => "InvalidExport",
            Self::PreconditionTiming => "PreconditionTiming",
            Self::CostOutsidePayment => "CostOutsidePayment",
            Self::InvalidCardinality => "InvalidCardinality",
            Self::ArithmeticArity => "ArithmeticArity",
            Self::InvalidDivisor => "InvalidDivisor",
            Self::EmptyBatch => "EmptyBatch",
            Self::InvalidEntry => "InvalidEntry",
            Self::CaptureTiming => "CaptureTiming",
            Self::InvalidRoles => "InvalidRoles",
            Self::InvalidModes => "InvalidModes",
            Self::DistributionRecipients => "DistributionRecipients",
            Self::InvalidMovement => "InvalidMovement",
        }
    }
}

/// A structural diagnostic containing only coordinates, never expression values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WellFormedError {
    /// Stable validation stage, distinct from lowering and capability admission.
    pub stage: &'static str,
    /// Exact reason for rejecting this program.
    pub reason: WellFormedReason,
    /// Nearest located node's source coordinates, if any.
    pub source: Option<Box<SourceSpan>>,
    /// Path inside the internal body.
    pub ir_path: String,
}
impl fmt::Display for WellFormedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} at {}",
            self.stage,
            self.reason.code(),
            self.ir_path
        )
    }
}
impl Error for WellFormedError {}

type Checked = Result<(), WellFormedError>;
type Key = (ScopeId, u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BindingKind {
    Value,
    Capture,
    Iteration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    value: TypeId,
    kind: BindingKind,
}
#[derive(Clone, Debug)]
struct Environment {
    scope: ScopeId,
    choices: bool,
    trigger: Option<super::EventSnapshotPolicy>,
    trigger_events: BTreeSet<Key>,
    play_modes: bool,
    play_payments: Option<ScopeId>,
    evolution_form: Option<Binding<ObjectRef>>,
    chooser: Option<PlayerRef>,
    private_choice: bool,
    choice_candidates: BTreeSet<Key>,
    distributions: BTreeMap<Key, super::RecipientSet>,
    access: BTreeMap<Key, super::Visibility>,
    receipt_access: BTreeMap<ReceiptId, super::Visibility>,
    bindings: BTreeMap<Key, Slot>,
    receipts: BTreeMap<ReceiptId, TypeId>,
}
impl Environment {
    fn inherit_values(&mut self, other: &Self) {
        self.bindings.extend(other.bindings.clone());
        self.receipts.extend(other.receipts.clone());
        self.access.extend(other.access.clone());
        self.receipt_access.extend(other.receipt_access.clone());
        self.distributions.extend(other.distributions.clone());
    }
}
#[derive(Clone)]
struct Checker {
    env: Environment,
    root: ScopeId,
    entry: bool,
    resolution_entry: Environment,
    play_entry: Option<Environment>,
    play_controller: Option<PlayerRef>,
    capture_time: Option<CaptureTime>,
    count_time: Option<super::SpellCountTime>,
    // Lives outside `Environment`, so no scope join can restore evidence a flip may have revoked.
    revoked: bool,
    scopes: BTreeSet<ScopeId>,
    ability_ids: BTreeSet<super::AbilityId>,
    effect_ids: BTreeSet<super::EffectDefinitionId>,
    bindings: BTreeSet<Key>,
    instructions: BTreeSet<InstructionId>,
    decisions: BTreeSet<super::DecisionId>,
    receipts: BTreeSet<ReceiptId>,
    source: Option<Box<SourceSpan>>,
    ability_source: Option<AbilitySource>,
    path: String,
}
#[expect(
    clippy::multiple_inherent_impl,
    reason = "Control and value rules share one checker state but live in separate modules."
)]
impl Checker {
    fn new(scope: ScopeId) -> Self {
        let env = Environment {
            scope,
            choices: true,
            trigger: None,
            trigger_events: BTreeSet::new(),
            play_modes: false,
            play_payments: None,
            evolution_form: None,
            chooser: None,
            private_choice: false,
            choice_candidates: BTreeSet::new(),
            distributions: BTreeMap::new(),
            access: BTreeMap::new(),
            receipt_access: BTreeMap::new(),
            bindings: BTreeMap::new(),
            receipts: BTreeMap::new(),
        };
        Self {
            resolution_entry: env.clone(),
            env,
            root: scope,
            entry: true,
            play_entry: None,
            play_controller: None,
            capture_time: None,
            count_time: None,
            revoked: false,
            scopes: BTreeSet::new(),
            ability_ids: BTreeSet::new(),
            effect_ids: BTreeSet::new(),
            bindings: BTreeSet::new(),
            instructions: BTreeSet::new(),
            decisions: BTreeSet::new(),
            receipts: BTreeSet::new(),
            source: None,
            ability_source: None,
            path: String::new(),
        }
    }

    fn error(&self, reason: WellFormedReason) -> WellFormedError {
        WellFormedError {
            stage: "well-formed",
            reason,
            source: self.source.clone(),
            ir_path: self.path.clone(),
        }
    }

    fn reject(&self) -> Checked {
        Err(self.error(WellFormedReason::UnsupportedShape))
    }

    fn at<T>(
        &mut self,
        field: &str,
        check: impl FnOnce(&mut Self) -> Result<T, WellFormedError>,
    ) -> Result<T, WellFormedError> {
        let length = self.path.len();
        self.path.push('/');
        self.path.push_str(field);
        let result = check(self);
        self.path.truncate(length);
        result
    }

    fn located<T>(
        &mut self,
        value: &Located<T>,
        check: impl FnOnce(&mut Self, &T) -> Checked,
    ) -> Checked {
        let previous = self.source.replace(Box::new(value.source.clone()));
        let result = check(self, &value.node);
        self.source = previous;
        result
    }

    fn isolated(&mut self, check: impl FnOnce(&mut Self) -> Checked) -> Checked {
        let previous = self.env.clone();
        let result = check(self);
        self.env = previous;
        result
    }

    fn define<T>(&mut self, binding: Binding<T>, kind: BindingKind) -> Checked
    where
        T: 'static,
    {
        let key = binding.key();
        if key.0 != self.env.scope {
            return Err(self.error(WellFormedReason::WrongScope));
        }
        if !self.bindings.insert(key) {
            return Err(self.error(WellFormedReason::DuplicateBinding));
        }
        self.env.bindings.insert(
            key,
            Slot {
                value: TypeId::of::<T>(),
                kind,
            },
        );
        Ok(())
    }

    fn reference<T>(&self, binding: Binding<T>, kind: BindingKind) -> Checked
    where
        T: 'static,
    {
        match self.env.bindings.get(&binding.key()) {
            None => Err(self.error(WellFormedReason::UnavailableBinding)),
            Some(slot)
                if *slot
                    != (Slot {
                        value: TypeId::of::<T>(),
                        kind,
                    }) =>
            {
                Err(self.error(WellFormedReason::BindingType))
            }
            Some(_) => Ok(()),
        }
    }

    fn receipt<T>(&self, reference: &ReceiptReference<T>) -> Checked
    where
        T: 'static,
    {
        match reference.location() {
            ReceiptLocation::Local { id } => match self.env.receipts.get(id) {
                None => Err(self.error(WellFormedReason::UnavailableReceipt)),
                Some(value) if *value != TypeId::of::<T>() => {
                    Err(self.error(WellFormedReason::ReceiptType))
                }
                Some(_) => Ok(()),
            },
            ReceiptLocation::Iteration { binding } => {
                self.reference(*binding, BindingKind::Iteration)
            }
        }
    }

    fn produce(&mut self, id: ReceiptId, value: TypeId) -> Checked {
        if !self.receipts.insert(id) {
            return Err(self.error(WellFormedReason::DuplicateReceipt));
        }
        self.env.receipts.insert(id, value);
        Ok(())
    }

    fn program(&mut self, program: &Program) -> Checked {
        if !self.scopes.insert(program.scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        self.env.scope = program.scope;
        self.at("steps", |checker| checker.program_steps(program))
    }

    fn node(&mut self, node: &Node) -> Checked {
        let play_payment = matches!(
            node,
            Node::Control {
                control: Control::Pay {
                    at: super::PaymentTime::PlayParameter,
                    ..
                }
            }
        );
        if !play_payment {
            self.env.play_payments = None;
        }
        // Play-time payment finishes before resolution starts, so leading captures may follow it.
        if !play_payment && !matches!(node, Node::Capture { .. }) {
            self.entry = false;
        }
        let result = match node {
            Node::Instruction { instruction } => {
                self.at("instruction", |checker| checker.instruction(instruction))
            }
            Node::Control { control } => self.at("control", |checker| checker.control(control)),
            Node::Capture { capture } => self.at("capture", |checker| checker.capture(capture)),
            Node::Decision { decision } => {
                self.at("decision", |checker| checker.decision(decision))
            }
        };
        if play_payment {
            self.resolution_entry = self.env.clone();
        }
        result
    }

    fn instruction(&mut self, instruction: &Instruction) -> Checked {
        self.instruction_in(instruction, InstructionContext::Effect, false)
    }

    /// Check an instruction whose start guard, if any, its decision or batch already evaluated.
    fn guarded_instruction(&mut self, instruction: &Instruction) -> Checked {
        self.instruction_in(instruction, InstructionContext::Effect, true)
    }

    fn instruction_in(
        &mut self,
        instruction: &Instruction,
        context: InstructionContext,
        guard_checked: bool,
    ) -> Checked {
        if !self.instructions.insert(instruction.id) {
            return Err(self.error(WellFormedReason::DuplicateInstruction));
        }
        if instruction.context != context {
            return Err(self.error(if context == InstructionContext::Cost {
                WellFormedReason::ExpectedCost
            } else {
                WellFormedReason::CostOutsidePayment
            }));
        }
        self.at("actor", |checker| checker.player(&instruction.actor))?;
        self.at("source", |checker| {
            checker.ability_source(&instruction.source)
        })?;
        if let Some(precondition) = &instruction.precondition {
            self.at("precondition", |checker| {
                if precondition.protects != instruction.id {
                    return Err(checker.error(WellFormedReason::PreconditionTiming));
                }
                // Rechecking would also redefine the predicate's candidate bindings.
                if guard_checked {
                    return Ok(());
                }
                checker.boolean(&precondition.predicate)
            })?;
        }
        // Payment belongs to a K.pay transaction; an effect has no all-or-nothing rule for it.
        if context == InstructionContext::Effect
            && matches!(
                instruction.action,
                Action::Resource {
                    operation: ResourceOperation::Pay,
                    ..
                }
            )
        {
            return self.reject();
        }
        let guarded_inputs = instruction
            .precondition
            .as_ref()
            .map(|_| (self.env.bindings.clone(), self.env.access.clone()));
        // The action publishes its output only after all of its inputs were checked.
        let previous_source = self.ability_source.replace(instruction.source.clone());
        let result = self.at("action", |checker| checker.action(&instruction.action));
        self.ability_source = previous_source;
        result?;
        if let Action::Discard { player, .. }
        | Action::Draw { player, .. }
        | Action::Search { player, .. } = &instruction.action
            && instruction.actor != *player
        {
            return self.reject();
        }
        self.produce(instruction.receipt, outcome(&instruction.action))?;
        self.observation(instruction);
        if let Some((bindings, access)) = guarded_inputs {
            // A skipped instruction has a receipt, but its output values were never produced.
            self.env.bindings = bindings;
            self.env.access = access;
        }
        Ok(())
    }

    fn ability_source(&mut self, source: &AbilitySource) -> Checked {
        if self
            .ability_source
            .as_ref()
            .is_some_and(|owner| owner != source)
        {
            return Err(self.error(WellFormedReason::InvalidEntry));
        }
        self.at("holder", |checker| checker.object(&source.holder))?;
        self.at("controller", |checker| checker.player(&source.controller))?;
        self.at("origin", |checker| checker.origin(&source.origin))
    }

    fn origin(&self, origin: &AbilityOrigin) -> Checked {
        match origin {
            AbilityOrigin::Printed { .. } => Ok(()),
            AbilityOrigin::Keyword { origin, .. } => self.origin(origin),
            AbilityOrigin::Granted { .. } => self.reject(),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Exhaustive action dispatch keeps unvalidated variants explicitly rejected."
    )]
    fn action(&mut self, action: &Action) -> Checked {
        match action {
            Action::Draw {
                player,
                count,
                mode,
            } => {
                self.stepwise(mode, player, super::StepwiseTiming::BeforeDraw)?;
                self.player(player)?;
                self.integer(count)
            }
            Action::Look {
                viewer,
                zone,
                count,
                mode,
            } => {
                self.stepwise(mode, viewer, super::StepwiseTiming::AfterLook)?;
                self.player(viewer)?;
                self.player(&zone.player)?;
                self.natural(count)
            }
            Action::Counter {
                holder,
                operation,
                amount,
                ..
            } => {
                self.holder(holder)?;
                if let CounterOperation::Move { to } = operation {
                    self.holder(to)?;
                }
                self.natural(amount)
            }
            Action::Destroy { objects }
            | Action::Banish { objects }
            | Action::Reveal { objects }
            | Action::FlipDown { objects }
            | Action::Posture { objects, .. } => self.selector(objects),
            Action::Dice {
                operation: DiceOperation::Roll {},
                result,
            } => self.define(*result, BindingKind::Value),
            Action::Declare { actor, declaration } => {
                if !self.env.choices {
                    return self.reject();
                }
                self.player(actor)?;
                self.declaration(declaration)
            }
            Action::Discard { player, mode } => self.discard(player, mode),
            Action::Damage {
                recipients,
                amount,
                flags,
                source,
                batch,
            } => self.damage(recipients, amount, *flags, source, *batch),
            Action::Token {
                tokens,
                destination,
                output,
                initial_state,
                entry_effects,
                ..
            } => {
                if !entry_effects.is_empty() {
                    return self.reject();
                }
                self.tokens(tokens, destination, initial_state.as_ref())?;
                self.define(*output, BindingKind::Value)
            }
            Action::Transfer {
                objects,
                destination_controller,
            } => {
                self.selector(objects)?;
                self.player(destination_controller)
            }
            Action::Life { player, change } => {
                self.player(player)?;
                match change {
                    super::NumericChange::Add { amount } | super::NumericChange::Set { amount } => {
                        self.integer(amount)
                    }
                }
            }
            Action::Resource { player, amount, .. } => {
                self.player(player)?;
                self.integer(amount)
            }
            Action::Evolve {
                subject,
                form,
                mode,
                link,
            } if *subject == (ObjectOperand::SourceHolder {})
                && *mode == super::EvolutionMode::Normal
                && matches!(&form.object, ObjectOperand::Bound { binding }
                        if self.env.evolution_form == Some(*binding)) =>
            {
                self.object(&form.object)?;
                self.define(*link, BindingKind::Value)
            }
            Action::Move {
                objects,
                movement,
                reason,
            } => {
                self.selector(objects)?;
                self.movement(movement, Some(*reason))
            }
            Action::Search {
                player,
                deck,
                candidate,
                filter,
                cardinality,
                groups,
                destination,
                continuation,
            } => self.search(
                player,
                deck,
                *candidate,
                filter,
                *cardinality,
                groups.as_ref(),
                destination.as_ref(),
                *continuation,
            ),
            Action::InstallEffect {
                subjects,
                capture,
                effect,
                duration,
            } => self.install(subjects, *capture, effect, duration),
            Action::RevealUntil { .. }
            | Action::Shuffle { .. }
            | Action::Order { .. }
            | Action::Evolve { .. }
            | Action::Transform { .. }
            | Action::FlipUp { .. }
            | Action::Modify { .. }
            | Action::Grant { .. }
            | Action::RemoveAbility { .. }
            | Action::Delay { .. }
            | Action::PlayCard { .. }
            | Action::PlayAbility { .. }
            | Action::MealRace { .. }
            | Action::RideDrive { .. }
            | Action::DriveCheck { .. }
            | Action::Equip { .. }
            | Action::Fusion { .. }
            | Action::Random { .. }
            | Action::Dice { .. }
            | Action::ScheduleTurn { .. }
            | Action::GameResult { .. }
            | Action::BoxPilot { .. } => self.reject(),
        }
    }

    fn declaration(&mut self, declaration: &Declaration) -> Checked {
        match declaration {
            Declaration::NonnegativeInteger { binding } => {
                self.define(*binding, BindingKind::Value)
            }
            Declaration::BoundedInteger {
                minimum,
                maximum,
                binding,
            } => {
                if minimum.0 > maximum.0 {
                    return Err(self.error(WellFormedReason::InvalidCardinality));
                }
                self.define(*binding, BindingKind::Value)
            }
            Declaration::CardNames { .. } => self.reject(),
        }
    }

    fn stepwise(
        &mut self,
        mode: &StepwiseMode,
        actor: &PlayerRef,
        timing: super::StepwiseTiming,
    ) -> Checked {
        let StepwiseMode::UpToStepwise { decision } = mode else {
            return Ok(());
        };
        if !self.env.choices {
            return self.reject();
        }
        if !self.decisions.insert(decision.id) {
            return Err(self.error(WellFormedReason::DuplicateDecision));
        }
        if decision.actor != *actor || decision.timing != timing {
            return Err(self.error(WellFormedReason::DecisionTiming));
        }
        if !matches!(&decision.visibility, super::Visibility::Only { players }
            if players.relative_to == *actor && players.relation == PlayerRelation::SelfPlayer
                && players.cardinality.minimum == Nat(1) && players.cardinality.maximum == Some(Nat(1)))
        {
            return self.reject();
        }
        self.player(&decision.actor)
    }

    fn capture(&mut self, capture: &Capture) -> Checked {
        match capture {
            Capture::Integer { at, value, into } => {
                let access = self.capture_access(*at, |checker| checker.integer(value));
                self.frozen(*at, |checker| checker.integer(value))?;
                self.define(into.binding, BindingKind::Capture)?;
                self.save_access(into.binding.key(), access);
                Ok(())
            }
            Capture::Natural { at, value, into } => {
                let access = self.capture_access(*at, |checker| checker.natural(value));
                self.frozen(*at, |checker| checker.natural(value))?;
                self.define(into.binding, BindingKind::Capture)?;
                self.save_access(into.binding.key(), access);
                Ok(())
            }
            Capture::Boolean { at, value, into } => {
                let access = self.capture_access(*at, |checker| checker.boolean(value));
                self.frozen(*at, |checker| checker.boolean(value))?;
                self.define(into.binding, BindingKind::Capture)?;
                self.save_access(into.binding.key(), access);
                Ok(())
            }
            Capture::Objects { at, value, into } => {
                let access = self.capture_access(*at, |checker| checker.selector(value));
                self.frozen(*at, |checker| checker.selector(value))?;
                self.define(into.binding, BindingKind::Capture)?;
                self.save_access(into.binding.key(), access);
                Ok(())
            }
            Capture::Names { .. }
            | Capture::Kind { .. }
            | Capture::Traits { .. }
            | Capture::Class { .. }
            | Capture::Title { .. }
            | Capture::NumberInfo { .. }
            | Capture::Player { .. }
            | Capture::Posture { .. }
            | Capture::Evolution { .. }
            | Capture::Abilities { .. }
            | Capture::Turn { .. }
            | Capture::Phase { .. }
            | Capture::Object { .. }
            | Capture::EntryPayment { .. }
            | Capture::ReceiptStatus { .. }
            | Capture::ReceiptMoves { .. }
            | Capture::Name { .. }
            | Capture::Zone { .. } => self.reject(),
        }
    }

    fn frozen(&mut self, at: CaptureTime, check: impl FnOnce(&mut Self) -> Checked) -> Checked {
        if let Some(expected) = self.capture_time {
            if at != expected {
                return Err(self.error(WellFormedReason::CaptureTiming));
            }
            return check(self);
        }
        match at {
            CaptureTime::AtResolution { scope } if scope != self.root => {
                Err(self.error(WellFormedReason::WrongScope))
            }
            // AtResolution freezes once on entering the execution. Only the root body's leading
            // captures run at that instant; a later node would observe the body's own changes.
            CaptureTime::AtResolution { .. } if !self.entry => self.reject(),
            // Leading captures freeze together, so none may depend on another.
            CaptureTime::AtResolution { .. } => {
                let current = self.env.clone();
                self.env = self.resolution_entry.clone();
                let result = check(self);
                self.env = current;
                result
            }
            CaptureTime::AtPlay { .. }
            | CaptureTime::AtSelection { .. }
            | CaptureTime::AtEvent { .. } => Err(self.error(WellFormedReason::CaptureTiming)),
        }
    }
}

// Nonprojectable results still occupy unique receipt slots.
struct OpaqueOutcome;
const fn outcome(action: &Action) -> TypeId {
    match action {
        Action::Discard { .. } => TypeId::of::<super::DiscardOutcome>(),
        Action::Draw { .. } => TypeId::of::<CardQuantityOutcome>(),
        Action::Look { .. } => TypeId::of::<LookOutcome>(),
        Action::Counter { .. } => TypeId::of::<CounterQuantityOutcome>(),
        Action::FlipDown { .. } => TypeId::of::<FlipOutcome>(),
        Action::Damage { .. } => TypeId::of::<super::DamageQuantityOutcome>(),
        Action::Token { .. } => TypeId::of::<super::TokenOutcome>(),
        Action::Move { .. } => TypeId::of::<super::MoveOutcome>(),
        Action::Destroy { .. } => TypeId::of::<super::DestroyOutcome>(),
        Action::Banish { .. } => TypeId::of::<super::BanishOutcome>(),
        Action::Search { .. } => TypeId::of::<super::SearchOutcome>(),
        Action::InstallEffect { .. }
        | Action::Transfer { .. }
        | Action::Life { .. }
        | Action::Reveal { .. }
        | Action::RevealUntil { .. }
        | Action::Shuffle { .. }
        | Action::Order { .. }
        | Action::Evolve { .. }
        | Action::Transform { .. }
        | Action::FlipUp { .. }
        | Action::Posture { .. }
        | Action::Resource { .. }
        | Action::Modify { .. }
        | Action::Grant { .. }
        | Action::RemoveAbility { .. }
        | Action::Delay { .. }
        | Action::PlayCard { .. }
        | Action::PlayAbility { .. }
        | Action::MealRace { .. }
        | Action::RideDrive { .. }
        | Action::DriveCheck { .. }
        | Action::Equip { .. }
        | Action::Fusion { .. }
        | Action::Random { .. }
        | Action::Dice { .. }
        | Action::Declare { .. }
        | Action::ScheduleTurn { .. }
        | Action::GameResult { .. }
        | Action::BoxPilot { .. } => TypeId::of::<OpaqueOutcome>(),
    }
}
