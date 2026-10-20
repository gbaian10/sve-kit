//! Structural validation of a closed program body, independent of execution support.

use super::{
    AbilityOrigin, AbilitySource, Action, Arithmetic, BatchOutcome, Binding, Bool, BoolRead,
    Capture, CaptureTime, CardQuantityOutcome, Cardinality, Control, CounterOperation,
    CounterQuantityOutcome, Declaration, DiceOperation, Expr, FlipOutcome, Holder, Instruction,
    InstructionBatch, InstructionContext, InstructionId, Int, IntFrozen, IntOperation, IntRead,
    IterationCheck, Located, LookOutcome, MeasuredReceipt, MeasuredReceiptList, Nat, NatFrozen,
    NatOperation, NatRead, Node, ObjectOperand, ObjectReceipt, ObjectReceiptList, ObjectRef,
    ObjectView, PlayerRef, PlayerRelation, PlayerSet, Predicate, Program, ReceiptExport, ReceiptId,
    ReceiptList, ReceiptLocation, ReceiptReference, RepeatBody, ScopeId, SelectionView, Selector,
    SourceSpan, StepwiseMode, Termination,
};
use alloc::collections::{BTreeMap, BTreeSet};
use core::any::TypeId;
use core::error::Error;
use core::fmt;

mod control;
mod decisions;
mod entries;
mod modes;
mod values;
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
    play_modes: bool,
    chooser: Option<PlayerRef>,
    private_choice: bool,
    choice_candidates: BTreeSet<Key>,
    public_scalars: BTreeSet<Key>,
    bindings: BTreeMap<Key, Slot>,
    receipts: BTreeMap<ReceiptId, TypeId>,
}
struct Checker {
    env: Environment,
    root: ScopeId,
    entry: bool,
    resolution_entry: Environment,
    play_entry: Option<Environment>,
    play_controller: Option<PlayerRef>,
    capture_time: Option<CaptureTime>,
    count_time: Option<super::SpellCountTime>,
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
            play_modes: false,
            chooser: None,
            private_choice: false,
            choice_candidates: BTreeSet::new(),
            public_scalars: BTreeSet::new(),
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

    fn at(&mut self, field: &str, check: impl FnOnce(&mut Self) -> Checked) -> Checked {
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
        if !matches!(node, Node::Capture { .. }) {
            self.entry = false;
        }
        match node {
            Node::Instruction { instruction } => {
                self.at("instruction", |checker| checker.instruction(instruction))
            }
            Node::Control { control } => self.at("control", |checker| checker.control(control)),
            Node::Capture { capture } => self.at("capture", |checker| checker.capture(capture)),
            Node::Decision { decision } => {
                self.at("decision", |checker| checker.decision(decision))
            }
        }
    }

    fn instruction(&mut self, instruction: &Instruction) -> Checked {
        self.instruction_in(instruction, InstructionContext::Effect)
    }

    fn instruction_in(
        &mut self,
        instruction: &Instruction,
        context: InstructionContext,
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
                checker.boolean(&precondition.predicate)
            })?;
        }
        if let Action::Discard { player, .. } = &instruction.action
            && instruction.actor != *player
        {
            return self.reject();
        }
        // The action publishes its output only after all of its inputs were checked.
        self.at("action", |checker| checker.action(&instruction.action))?;
        self.produce(instruction.receipt, outcome(&instruction.action))
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
            Action::Reveal { objects }
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
            Action::Discard { player, mode } => self.discard(player, mode),
            Action::InstallEffect { .. }
            | Action::Move { .. }
            | Action::Destroy { .. }
            | Action::Banish { .. }
            | Action::Transfer { .. }
            | Action::Damage { .. }
            | Action::Life { .. }
            | Action::Search { .. }
            | Action::RevealUntil { .. }
            | Action::Shuffle { .. }
            | Action::Order { .. }
            | Action::Evolve { .. }
            | Action::Token { .. }
            | Action::Transform { .. }
            | Action::FlipUp { .. }
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
            | Action::ScheduleTurn { .. }
            | Action::GameResult { .. }
            | Action::BoxPilot { .. } => self.reject(),
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
                self.frozen(*at, |checker| checker.integer(value))?;
                self.define(into.binding, BindingKind::Capture)
            }
            Capture::Natural { at, value, into } => {
                self.frozen(*at, |checker| checker.natural(value))?;
                self.define(into.binding, BindingKind::Capture)
            }
            Capture::Boolean { at, value, into } => {
                self.frozen(*at, |checker| checker.boolean(value))?;
                self.define(into.binding, BindingKind::Capture)
            }
            Capture::Objects { at, value, into } => {
                self.frozen(*at, |checker| checker.selector(value))?;
                self.define(into.binding, BindingKind::Capture)
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
        Action::InstallEffect { .. }
        | Action::Move { .. }
        | Action::Destroy { .. }
        | Action::Banish { .. }
        | Action::Transfer { .. }
        | Action::Damage { .. }
        | Action::Life { .. }
        | Action::Search { .. }
        | Action::Reveal { .. }
        | Action::RevealUntil { .. }
        | Action::Shuffle { .. }
        | Action::Order { .. }
        | Action::Evolve { .. }
        | Action::Token { .. }
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
