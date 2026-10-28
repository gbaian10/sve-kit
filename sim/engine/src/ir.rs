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

macro_rules! record {
    ($(#[$meta:meta])* $name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[doc = concat!("Internal IR ", stringify!($name), ".")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name { $(#[doc = stringify!($field)] pub $field: $ty),* }
        impl super::Walk for $name {
            fn walk(&self, visitor: &mut super::Visitor, path: &str) {
                $(self.$field.walk(visitor, &format!("{path}/{}", stringify!($field)));)*
            }
        }
    };
}

macro_rules! variants {
    ($(#[$meta:meta])* $name:ident { $($variant:ident { $($field:ident : $ty:ty),* $(,)? } => [$($code:literal),*]),* $(,)? }) => {
        $(#[$meta])*
        #[doc = concat!("Closed internal IR ", stringify!($name), ".")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum $name { $(#[doc = stringify!($variant)] $variant { $(#[doc = stringify!($field)] $field: $ty),* }),* }
        impl super::Walk for $name {
            fn walk(&self, visitor: &mut super::Visitor, path: &str) {
                match self {
                    $(Self::$variant { $($field),* } => {
                        visitor.require(concat!("shape.", stringify!($name), ".", stringify!($variant)), path);
                        $(visitor.require($code, path);)*
                        $($field.walk(visitor, &format!("{path}/{}", stringify!($field)));)*
                    }),*
                }
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
            fn walk(&self, visitor: &mut super::Visitor, path: &str) {
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

mod actions;
mod answers;
mod capabilities;
mod effects;
pub(crate) mod events;
mod flow;
pub mod lowering;
pub(crate) mod optional_payment;
pub(crate) mod printed_keywords;
mod projections;
pub(crate) mod quick;
mod runtime;
mod values;
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

use capabilities::Visitor;

trait Walk {
    fn walk(&self, visitor: &mut Visitor, path: &str);
}

impl<T: Walk> Walk for Box<T> {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        self.as_ref().walk(visitor, path);
    }
}
impl<T: Walk> Walk for Option<T> {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        if let Some(value) = self {
            value.walk(visitor, path);
        }
    }
}
impl<T: Walk> Walk for Vec<T> {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        for (index, value) in self.iter().enumerate() {
            value.walk(visitor, &format!("{path}/{index}"));
        }
    }
}

#[cfg(test)]
mod tests;
