//! Read-only structural traversal; callbacks never establish execution or read permission.

use super::{
    Ability, Action, CardProgram, Control, CostSpec, Effect, Instruction, Located, ObjectOperand,
    Program, SourceSpan, Walk as _,
};

/// Typed structural positions, paired on entry and exit at the same IR path.
#[derive(Clone, Copy, Debug)]
pub enum NodeRef<'node> {
    /// A source override for a located subtree; restore the parent on exit.
    Source(&'node SourceSpan),
    /// A lexical program scope, including empty bodies.
    Program(&'node Program),
    /// An independent ability definition, including granted definitions.
    Ability(&'node Ability),
    /// Every control family, with its branch, iteration and payment metadata.
    Control(&'node Control),
    /// An instruction in a body, batch or cost specification.
    Instruction(&'node Instruction),
    /// An action, including delayed bodies and granted abilities.
    Action(&'node Action),
    /// A continuous, replacement or other declaration.
    Effect(&'node Effect),
    /// Ordered or alternative costs, separate from the paid continuation.
    CostSpec(&'node CostSpec),
    /// An object operand, including instruction provenance and nested expressions.
    ObjectOperand(&'node ObjectOperand),
}

/// An observer's traversal choice, not an admission decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Traversal {
    /// Observe every descendant in declaration order.
    Descend,
    /// Omit this subtree's descendants, keeping the matching exit notification.
    Skip,
}

/// Observes structure without combining branch environments or granting read access.
/// Paths retain the capability inventory's relative field/index coordinates.
pub trait Visitor {
    /// Called before any descendants, including on empty scopes.
    fn enter(&mut self, _node: NodeRef<'_>, _path: &str) -> Traversal {
        Traversal::Descend
    }

    /// Called after descendants, or immediately after a skipped entry.
    fn leave(&mut self, _node: NodeRef<'_>, _path: &str) {}

    /// Reports an existing closed capability code at its exact structural position.
    fn require(&mut self, _code: &'static str, _path: &str) {}
}

/// Visit a program under a caller-supplied relative prefix; every Located node
/// supplies its own source override. No well-formedness or admission is implied.
pub fn program<Observer>(program: &Program, path: &str, visitor: &mut Observer)
where
    Observer: Visitor,
{
    program.walk(visitor, path);
}

/// Visit a complete card, including provenance, play costs and granted definitions.
pub fn card<Observer>(card: &Located<CardProgram>, visitor: &mut Observer)
where
    Observer: Visitor,
{
    card.walk(visitor, "");
}

/// Visit one independent definition, including declarations and nested abilities.
pub fn ability<Observer>(ability: &Located<Ability>, visitor: &mut Observer)
where
    Observer: Visitor,
{
    ability.walk(visitor, "");
}
