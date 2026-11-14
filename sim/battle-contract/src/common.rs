//! Seats, host-issued codes and references shared by every message.

use crate::number::Nat;
use alloc::borrow::Cow;
use core::fmt;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// Wire value of [`Format`].
const FORMAT: u64 = 1;

/// The single wire format of one build; it travels as the number `1`.
///
/// Worker and client come from the same build, so any other value means a reload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Format;

impl Serialize for Format {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(FORMAT)
    }
}

impl<'de> Deserialize<'de> for Format {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_u64(FormatVisitor)
    }
}

impl JsonSchema for Format {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Format")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({ "const": FORMAT })
    }
}

struct FormatVisitor;

impl Visitor<'_> for FormatVisitor {
    type Value = Format;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the wire format number 1")
    }

    fn visit_u64<E>(self, v: u64) -> Result<Format, E>
    where
        E: de::Error,
    {
        if v == FORMAT {
            Ok(Format)
        } else {
            Err(E::invalid_value(de::Unexpected::Unsigned(v), &self))
        }
    }
}

/// A player seat.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Seat {
    /// First seat.
    P1,
    /// Second seat.
    P2,
}

macro_rules! opaque {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wrap a code issued by the host.
            #[must_use]
            pub const fn new(code: String) -> Self {
                Self(code)
            }

            /// The code as issued, for comparison only.
            #[must_use]
            pub const fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }
    };
}

opaque! {
    /// A viewer's reference to one object while it stays identifiable.
    ///
    /// It retires when identification is lost and never names a physical identity.
    ObjectHandle
}
opaque! {
    /// One question occurrence, bound to its seat, native token and shape.
    QuestionId
}
opaque! {
    /// One candidate, valid only within the question that lists it.
    OptionId
}
opaque! {
    /// A client's label pairing one submission with its receipt.
    RequestId
}

/// A card face in the snapshot pinned by this game.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct CardRef {
    /// Permanent printing identifier, never derived from a name or prefix.
    pub printing_id: String,
    /// Face index within that printing.
    pub face: Nat,
}

/// Content pinned by identifier and digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContentRef {
    /// Published identifier of the content.
    pub id: String,
    /// Lowercase hexadecimal SHA-256 of its bytes.
    pub sha256: String,
}

/// A position in printed card text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextRef {
    /// Card face whose wording is referenced.
    pub card: CardRef,
    /// Revision of that wording in the pinned snapshot.
    pub wording_revision: String,
    /// Printed section containing the line.
    pub section: String,
    /// Line within the section.
    pub line: Nat,
    /// Step within the line; only for an exact correspondence to the wording.
    pub step: Option<Nat>,
    /// Card that granted this text, when it is not printed on its holder.
    pub granted_by: Option<CardRef>,
    /// Current holder; absent once the viewer can no longer identify it.
    pub holder: Option<ObjectHandle>,
}

/// Where a rule reference comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    /// Comprehensive rules.
    Cr,
    /// Official question and answer.
    Qa,
    /// Recorded ruling.
    Ruling,
}

/// How firmly a rule reference supports its use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleStrength {
    /// Stated by an official source.
    Official,
    /// Generalised from official cases.
    Generalized,
    /// Inferred by the project.
    Inferred,
    /// Not yet decided.
    Undecided,
}

/// A rule, answer or ruling that an item relies on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleRef {
    /// Kind of source.
    pub kind: RuleSource,
    /// Identifier within that source.
    pub id: String,
    /// Revision of the source.
    pub revision: String,
    /// Strength of the support.
    pub strength: RuleStrength,
}

/// A rules zone of one player.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    /// Main deck.
    Deck,
    /// Evolve deck.
    EvolveDeck,
    /// Hand.
    Hand,
    /// Field.
    Field,
    /// EX area.
    Ex,
    /// Cemetery.
    Cemetery,
    /// Banished cards.
    Banish,
    /// Resolving cards.
    Resolution,
    /// Evolve cards linked to field cards.
    Evolution,
    /// Race area.
    Race,
    /// Drive area.
    Drive,
    /// Trigger area.
    Trigger,
    /// Equipment area.
    Equipment,
}

/// Something an item is about: a player, a leader or a known card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Subject {
    /// A player as a rules participant.
    Player {
        /// That player's seat.
        player: Seat,
    },
    /// A player's leader, which is seat appearance and not an object.
    Leader {
        /// The leader's seat.
        player: Seat,
    },
    /// A card whose face this viewer is authorized to know.
    Card {
        /// Authorized face.
        card: CardRef,
        /// Current object, only while the viewer can still identify it.
        object: Option<ObjectHandle>,
    },
}

/// Where an effect comes from, as far as this viewer is authorized to know.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceRef {
    /// An exact position in printed text.
    Text {
        /// Referenced wording.
        text: TextRef,
    },
    /// A known printing whose exact text is not available.
    ///
    /// The face and the line stay unknown rather than guessed.
    Card {
        /// Printing supplying the effect.
        printing_id: String,
        /// Face within that printing, when known.
        face: Option<Nat>,
        /// Printed line, when the effect comes from one.
        line: Option<Nat>,
        /// Current holder; absent once the viewer can no longer identify it.
        holder: Option<ObjectHandle>,
    },
}
