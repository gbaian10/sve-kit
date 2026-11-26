use super::walk::{NodeRef, Traversal, Visitor};
use super::{CardProgram, Located, SourceSpan, Walk};
use alloc::collections::{BTreeMap, BTreeSet};
use core::mem::replace;
use core::num::NonZeroU32;

/// An exact semantic capability version, separate from transport capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Capability {
    /// Closed derivation code; versions never imply support for older versions.
    pub code: &'static str,
    /// Positive semantic version.
    pub version: NonZeroU32,
}
/// One occurrence of a semantic requirement.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    /// Internal IR node path.
    pub ir_path: String,
    /// Expanded author's source coordinate.
    pub source: SourceSpan,
}
/// A requirement merges occurrences, but never versions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    /// Required exact pair.
    pub capability: Capability,
    /// Nonempty, ordered, unique occurrences.
    pub locations: BTreeSet<Location>,
}
/// Deterministic requirements of an entire expanded internal program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirements {
    /// Sorted exact capability pairs with all triggering locations.
    pub required: Vec<Requirement>,
}

struct RequirementsVisitor {
    source: SourceSpan,
    sources: Vec<SourceSpan>,
    required: BTreeMap<Capability, BTreeSet<Location>>,
}
impl Visitor for RequirementsVisitor {
    fn enter(&mut self, node: NodeRef<'_>, _path: &str) -> Traversal {
        if let NodeRef::Source(source) = node {
            self.sources.push(replace(&mut self.source, source.clone()));
        }
        Traversal::Descend
    }

    fn leave(&mut self, node: NodeRef<'_>, _path: &str) {
        if let NodeRef::Source(_) = node
            && let Some(source) = self.sources.pop()
        {
            self.source = source;
        }
    }

    fn require(&mut self, code: &'static str, path: &str) {
        self.required
            .entry(Capability {
                code,
                version: NonZeroU32::MIN,
            })
            .or_default()
            .insert(Location {
                ir_path: path.to_owned(),
                source: self.source.clone(),
            });
    }
}
impl Requirements {
    /// Derive every requirement of a closed internal execution body.
    #[must_use]
    pub fn derive_program(program: &Located<super::Program>) -> Self {
        Self::walked(program)
    }

    /// Visit every nested branch, even one whose runtime condition is false.
    #[must_use]
    pub fn derive(program: &Located<CardProgram>) -> Self {
        Self::walked(program)
    }

    fn walked<T>(program: &Located<T>) -> Self
    where
        T: Walk,
    {
        let mut visitor = RequirementsVisitor {
            source: program.source.clone(),
            sources: vec![],
            required: BTreeMap::new(),
        };
        program.walk(&mut visitor, "");
        Self {
            required: visitor
                .required
                .into_iter()
                .map(|(capability, locations)| Requirement {
                    capability,
                    locations,
                })
                .collect(),
        }
    }
}
