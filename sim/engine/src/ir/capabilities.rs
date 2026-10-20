use super::{CardProgram, Located, SourceSpan, Walk as _};
use alloc::collections::{BTreeMap, BTreeSet};
use core::num::NonZeroU32;
use sha2::{Digest as _, Sha256};

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
/// Source-pinned identity for the internal schema or derivation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Internal identifier, not a public schema URI.
    pub id: &'static str,
    /// SHA-256 of implementation bytes.
    pub hash: String,
}
/// Deterministic requirements of an entire expanded internal program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirements {
    /// Rules that derived this set.
    pub derivation: Identity,
    /// Sorted exact capability pairs with all triggering locations.
    pub required: Vec<Requirement>,
}
/// Closed engine support declaration. It is not serialized or published.
#[expect(
    clippy::partial_pub_fields,
    reason = "Version pins are inspectable; executable support cannot be extended by callers."
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registry {
    /// Author language pinned by this engine.
    pub dsl: &'static str,
    /// Source-pinned internal type model.
    pub ir_schema: Identity,
    /// Engine implementation pin for this internal admission layer.
    pub engine_build: String,
    /// Source-pinned capability derivation.
    pub derivation: Identity,
    supported: BTreeSet<Capability>,
}
/// Atomic rejection: a failed program never yields a partial executable body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    /// The registry belongs to another language, type model, build or derivation.
    StaleRegistry(Vec<DependencyMismatch>),
    /// All missing capabilities and their author/IR locations.
    Missing(Vec<Requirement>),
}

pub(super) struct Visitor {
    pub(super) source: SourceSpan,
    required: BTreeMap<Capability, BTreeSet<Location>>,
}
impl Visitor {
    pub(super) fn require(&mut self, code: &'static str, path: &str) {
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
    /// Visit every nested branch, even one whose runtime condition is false.
    #[must_use]
    pub fn derive(program: &Located<CardProgram>) -> Self {
        let mut visitor = Visitor {
            source: program.source.clone(),
            required: BTreeMap::new(),
        };
        program.walk(&mut visitor, "");
        Self {
            derivation: derivation_identity(),
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
impl Registry {
    /// No typed executor is wired yet; the prototype does not prove D0 support.
    #[must_use]
    pub fn current() -> Self {
        let schema = schema_identity();
        let derivation = derivation_identity();
        Self {
            dsl: "1.0",
            engine_build: format!(
                "sve-engine/{}/{}",
                env!("CARGO_PKG_VERSION"),
                derivation.hash
            ),
            ir_schema: schema,
            derivation,
            supported: BTreeSet::new(),
        }
    }

    /// Read the closed support set without extending it at runtime.
    #[must_use]
    pub const fn supported(&self) -> &BTreeSet<Capability> {
        &self.supported
    }

    /// Check every ability against exact versions and current identities.
    /// This is not scope validation or permission to execute the unvalidated tree.
    ///
    /// # Errors
    /// Returns stale metadata or every missing semantic requirement.
    pub fn check_capabilities(
        &self,
        program: &Located<CardProgram>,
    ) -> Result<Requirements, AdmissionError> {
        let current = Self::current();
        let mut stale = Vec::new();
        for (dependency, expected, actual) in [
            ("dsl", current.dsl, self.dsl),
            ("ir_schema.id", current.ir_schema.id, self.ir_schema.id),
            (
                "ir_schema.hash",
                current.ir_schema.hash.as_str(),
                self.ir_schema.hash.as_str(),
            ),
            (
                "engine_build",
                current.engine_build.as_str(),
                self.engine_build.as_str(),
            ),
            ("derivation.id", current.derivation.id, self.derivation.id),
            (
                "derivation.hash",
                current.derivation.hash.as_str(),
                self.derivation.hash.as_str(),
            ),
        ] {
            if expected != actual {
                stale.push(DependencyMismatch {
                    dependency,
                    expected: expected.to_owned(),
                    actual: actual.to_owned(),
                });
            }
        }
        if !stale.is_empty() {
            return Err(AdmissionError::StaleRegistry(stale));
        }
        let requirements = Requirements::derive(program);
        let missing: Vec<_> = requirements
            .required
            .iter()
            .filter(|requirement| !self.supported.contains(&requirement.capability))
            .cloned()
            .collect();
        if missing.is_empty() {
            Ok(requirements)
        } else {
            Err(AdmissionError::Missing(missing))
        }
    }

    #[cfg(test)]
    pub(super) fn test_support(program: &Located<CardProgram>) -> Self {
        let mut registry = Self::current();
        registry.supported = Requirements::derive(program)
            .required
            .into_iter()
            .map(|item| item.capability)
            .collect();
        registry
    }
    #[cfg(test)]
    pub(super) fn test_remove(&mut self, code: &str) {
        self.supported.retain(|capability| capability.code != code);
    }
    #[cfg(test)]
    pub(super) fn test_upgrade(&mut self, code: &'static str) {
        self.test_remove(code);
        self.supported.insert(Capability {
            code,
            version: NonZeroU32::new(2).unwrap_or(NonZeroU32::MIN),
        });
    }
}

fn schema_identity() -> Identity {
    let mut digest = Sha256::new();
    for source in [
        include_str!("../ir.rs"),
        include_str!("values.rs"),
        include_str!("actions.rs"),
        include_str!("flow.rs"),
        include_str!("effects.rs"),
        include_str!("events.rs"),
        include_str!("runtime.rs"),
        include_str!("projections.rs"),
        include_str!("values/sealed.rs"),
    ] {
        digest.update(source.as_bytes());
    }
    Identity {
        id: "internal:d0-types:1",
        hash: hex_digest(digest.finalize().as_slice()),
    }
}
fn derivation_identity() -> Identity {
    let mut digest = Sha256::new();
    digest.update(schema_identity().hash.as_bytes());
    digest.update(include_bytes!("capabilities.rs"));
    Identity {
        id: "internal:d0-derivation:1",
        hash: hex_digest(digest.finalize().as_slice()),
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4_u8, byte & 15])
        .map(|digit| {
            char::from(if digit < 10 {
                b'0'.saturating_add(digit)
            } else {
                b'a'.saturating_add(digit.saturating_sub(10))
            })
        })
        .collect()
}

/// One stale dependency, retained without hiding other mismatches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyMismatch {
    /// Registry dependency name.
    pub dependency: &'static str,
    /// Identity compiled into this engine.
    pub expected: String,
    /// Identity supplied by the caller.
    pub actual: String,
}
