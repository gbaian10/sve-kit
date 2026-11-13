//! Internal D0 vocabulary and conservative semantic admission.
//!
//! These types have no wire encoding and are not accepted by the prototype
//! executor. Public programs remain disabled (author contract 1.0, §6–7).
//!
//! Signed requests cannot silently become nonnegative quantities:
//! ```compile_fail
//! use sve_engine::ir::{Expr, Int, Nat};
//! let requested: Expr<Int> = Expr::Literal(Int(-2));
//! let actual: Expr<Nat> = requested;
//! ```
//!
//! Runtime object generations cannot be embedded as program literals:
//! ```compile_fail
//! use sve_engine::ir::{Expr, Generation, ObjectId, ObjectRef};
//! let object: Expr<ObjectRef> = Expr::Literal(ObjectRef {
//!     id: ObjectId(1), generation: Generation(1),
//! });
//! ```
//!
//! Search discovery does not prove that an object moved:
//! ```compile_fail
//! use sve_engine::ir::{Binding, ScopeId, SearchFound, Selector};
//! let found: Binding<SearchFound> = Binding::new(1, ScopeId(1));
//! let moved = Selector::Bound { reference: found };
//! ```
//!
//! A quantity receipt cannot be followed as a search movement:
//! ```compile_fail
//! use sve_engine::ir::{CardQuantityOutcome, MovementReceipt, ReceiptId, ReceiptReference};
//! let draw: ReceiptReference<CardQuantityOutcome> = ReceiptReference::new(ReceiptId(1));
//! let search = MovementReceipt::Search { receipt: draw };
//! ```

//!
//! Legal counterparts keep the compile-fail examples tied to usable public types:
//! ```rust
//! use sve_engine::ir::*;
//! let signed: Expr<Int> = Expr::Literal(Int(-2));
//! let natural: Expr<Nat> = Expr::Literal(Nat(2));
//! let physical = ObjectRef { id: ObjectId(1), generation: Generation(1) };
//! let symbolic: Expr<ObjectRef> = Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {}));
//! let objects = Selector::Bound { reference: Binding::<Selector>::new(1, ScopeId(1)) };
//! let movement = MovementReceipt::Search { receipt: ReceiptReference::<SearchOutcome>::new(ReceiptId(1)) };
//! let followed = MovedRef { receipt: movement, member: Binding::new(2, ScopeId(1)), permission: FollowPermission::SameEffectExplicitReference {} };
//! ```
//!
//! Individual follow references enforce the same movement boundary as collections:
//! ```compile_fail
//! use sve_engine::ir::*;
//! let draw: ReceiptReference<CardQuantityOutcome> = ReceiptReference::new(ReceiptId(1));
//! let followed = MovedRef { receipt: draw, member: Binding::new(2, ScopeId(1)), permission: FollowPermission::SameEffectExplicitReference {} };
//! ```

macro_rules! visit_children {
    ([], $visitor:ident, $path:ident, $node:ident, $body:block) => {
        $body
    };
    ([$name:ident], $visitor:ident, $path:ident, $node:ident, $body:block) => {{
        if $visitor.enter(super::walk::NodeRef::$name($node), $path)
            == super::walk::Traversal::Descend
        {
            $body
        }
        $visitor.leave(super::walk::NodeRef::$name($node), $path);
    }};
}

macro_rules! record {
    (@visit $(#[$meta:meta])* $name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        record!(@impl [$name] $(#[$meta])* $name { $($field: $ty),* });
    };
    ($(#[$meta:meta])* $name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        record!(@impl [] $(#[$meta])* $name { $($field: $ty),* });
    };
    (@impl [$($visit:ident)?] $(#[$meta:meta])* $name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[doc = concat!("Internal IR ", stringify!($name), ".")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name { $(#[doc = stringify!($field)] pub $field: $ty),* }
        impl super::Walk for $name {
            fn walk(&self, visitor: &mut impl super::Visitor, path: &str) {
                visit_children!([$($visit)?], visitor, path, self, {
                    $(self.$field.walk(visitor, &format!("{path}/{}", stringify!($field)));)*
                });
            }
        }
    };
}

macro_rules! variants {
    (@visit $(#[$meta:meta])* $name:ident { $($variant:ident { $($field:ident : $ty:ty),* $(,)? } => [$($code:literal),*]),* $(,)? }) => {
        variants!(@impl [$name] $(#[$meta])* $name { $($variant { $($field: $ty),* } => [$($code),*]),* });
    };
    ($(#[$meta:meta])* $name:ident { $($variant:ident { $($field:ident : $ty:ty),* $(,)? } => [$($code:literal),*]),* $(,)? }) => {
        variants!(@impl [] $(#[$meta])* $name { $($variant { $($field: $ty),* } => [$($code),*]),* });
    };
    (@impl [$($visit:ident)?] $(#[$meta:meta])* $name:ident { $($variant:ident { $($field:ident : $ty:ty),* $(,)? } => [$($code:literal),*]),* $(,)? }) => {
        $(#[$meta])*
        #[doc = concat!("Closed internal IR ", stringify!($name), ".")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum $name { $(#[doc = stringify!($variant)] $variant { $(#[doc = stringify!($field)] $field: $ty),* }),* }
        impl super::Walk for $name {
            fn walk(&self, visitor: &mut impl super::Visitor, path: &str) {
                visit_children!([$($visit)?], visitor, path, self, { match self {
                    $(Self::$variant { $($field),* } => {
                        visitor.require(concat!("shape.", stringify!($name), ".", stringify!($variant)), path);
                        $(visitor.require($code, path);)*
                        $($field.walk(visitor, &format!("{path}/{}", stringify!($field)));)*
                    }),*
                }});
            }
        }
    };
}

macro_rules! tags {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[doc = concat!("Closed IR ", stringify!($name), ".")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum $name { $(#[doc = stringify!($variant)] $variant),* }
        impl super::Walk for $name {
            fn walk(&self, visitor: &mut impl super::Visitor, path: &str) {
                let code = match self { $(Self::$variant => concat!("internal.", stringify!($name), ".", stringify!($variant))),* };
                visitor.require(code, path);
            }
        }
    };
}

macro_rules! runtime_record {
    ($(#[$meta:meta])* $name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[doc = concat!("Runtime record ", stringify!($name), ". Never embedded in a program.")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name { $(#[doc = stringify!($field)] pub $field: $ty),* }
    };
}
macro_rules! runtime_variants {
    ($(#[$meta:meta])* $name:ident { $($variant:ident { $($field:ident : $ty:ty),* $(,)? }),* $(,)? }) => {
        $(#[$meta])*
        #[doc = concat!("Runtime record ", stringify!($name), ". Never embedded in a program.")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum $name { $(#[doc = stringify!($variant)] $variant { $(#[doc = stringify!($field)] $field: $ty),* }),* }
    };
}

pub(crate) mod ability_play;
pub(crate) mod ability_suppression;
mod actions;
pub(crate) mod advance_entry;
mod answers;
pub(crate) mod attack_restrictions;
mod capabilities;
pub(crate) mod choice_rules;
pub(crate) mod delayed;
pub(crate) mod dice;
pub(crate) mod effect_evolution;
mod effects;
pub(crate) mod events;
pub(crate) mod evolution_destination;
pub(crate) mod evolution_limits;
mod flow;
pub(crate) mod fusion;
pub(crate) mod granted_cost;
pub(crate) mod history_turn;
pub mod lowering;
pub(crate) mod names;
pub(crate) mod nested_play;
pub(crate) mod number_bounds;
pub(crate) mod observation;
pub(crate) mod observation_selection;
pub(crate) mod optional_payment;
pub(crate) mod perspective;
pub(crate) mod play_restrictions;
pub(crate) mod player_stages;
pub(crate) mod prevention;
pub(crate) mod printed_keywords;
mod projections;
pub(crate) mod quick;
pub(crate) mod receipt_reads;
pub(crate) mod removal_restrictions;
pub(crate) mod reveal_cost;
pub(crate) mod reveal_until;
pub(crate) mod ride;
mod runtime;
pub(crate) mod shuffle;
pub(crate) mod stand_restrictions;
pub(crate) mod static_declarations;
pub(crate) mod trigger_restrictions;
mod values;
pub(crate) mod victory;
pub mod walk;
mod wellformed;

pub use actions::*;
pub use answers::*;
pub use capabilities::{
    AdmissionError, Capability, DependencyMismatch, Identity, Location, Registry, Requirement,
    Requirements,
};
pub use effects::*;
pub use events::*;
pub use flow::*;
pub use projections::*;
pub use runtime::*;
pub use values::*;
pub use wellformed::{
    WellFormedCardProgram, WellFormedError, WellFormedProcedure, WellFormedProgram,
    WellFormedReason,
};

use walk::Visitor;

trait Walk {
    fn walk(&self, visitor: &mut impl Visitor, path: &str);
}

impl<T: Walk> Walk for Box<T> {
    fn walk(&self, visitor: &mut impl Visitor, path: &str) {
        self.as_ref().walk(visitor, path);
    }
}
impl<T: Walk> Walk for Option<T> {
    fn walk(&self, visitor: &mut impl Visitor, path: &str) {
        if let Some(value) = self {
            value.walk(visitor, path);
        }
    }
}
impl<T: Walk> Walk for Vec<T> {
    fn walk(&self, visitor: &mut impl Visitor, path: &str) {
        for (index, value) in self.iter().enumerate() {
            value.walk(visitor, &format!("{path}/{index}"));
        }
    }
}

#[cfg(test)]
mod tests;

pub(crate) mod public_evolve;
pub(crate) mod public_numbers;

pub(crate) mod exchange;
pub(crate) mod x_pp;

pub(crate) mod follow_keyword;

pub(crate) mod mixed_ex;

pub(crate) mod hand_position;
