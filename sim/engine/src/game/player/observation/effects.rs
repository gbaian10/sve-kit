//! Installed effects expressed without authority identities or executable text.

use super::super::{ObjectHandle, Viewer};
use super::{EffectRecipient, PreventionChange, TriggerTiming};
use crate::ir::{AbilityKind, CostKind, DamageKind};

/// Public text origin, with a link only during uninterrupted identification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectSource {
    /// Catalog card supplying the text.
    pub card: String,
    /// Printed face coordinate within the source card, when known.
    pub face: Option<String>,
    /// Printed line, when the effect comes from an ability.
    pub line: Option<u64>,
    /// Currently identifiable producing holder; absent after identification is lost.
    pub holder: Option<ObjectHandle>,
}

/// Temporal boundary with no private clock or physical identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectExpiry {
    /// No temporal expiry; ordinary zone rules still apply.
    NoTemporalExpiry,
    /// End of the current resolution.
    ResolutionEnd,
    /// End of a turn that has already begun.
    TurnEnd,
    /// End of this player's next turn, which has not begun yet.
    NextTurnEnd(Viewer),
    /// When the identified source leaves the field.
    SourceLeavesField,
    /// End of this player's next rule-start procedure.
    NextStartEnd(Viewer),
}

/// A repeated active interval, separate from the effect's principal lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectInterval {
    /// Waiting for this player's next turn.
    NextTurn(Viewer),
    /// Active during the current turn.
    CurrentTurn,
}

/// Frozen numeric operation, before other layers and damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberChange {
    /// Add a signed amount.
    Add(i64),
    /// Set a value.
    Set(i64),
}

/// A granted definition's public role, without its program or runtime instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantedDefinition {
    /// Ability category.
    pub kind: AbilityKind,
    /// Printed line coordinate.
    pub line: u64,
}

/// Supported installed changes; current effective values remain in card properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstalledChange {
    /// Frozen attack and maximum-life layers.
    Numeric {
        /// Attack operation, if any.
        attack: Option<NumberChange>,
        /// Maximum-life operation, if any; Set includes the damage captured at installation.
        life: Option<NumberChange>,
    },
    /// Cost layer; the contextual quote is separate from these frozen operands.
    Cost {
        /// Play or evolution cost.
        kind: CostKind,
        /// Frozen cost operation.
        change: NumberChange,
    },
    /// Keyword, trait and definition grants.
    Grant {
        /// Granted keyword names.
        keywords: Vec<String>,
        /// Granted trait names.
        traits: Vec<String>,
        /// Granted ability roles.
        definitions: Vec<GrantedDefinition>,
    },
    /// Earlier abilities were removed; later grants can remain effective.
    AbilityRemoval {
        /// Box also restricts posture and combat.
        boxed: bool,
    },
    /// An amulet temporarily acts as a follower.
    Pilot,
    /// The next rule-start procedure cannot stand this card.
    NoStand {
        /// Player whose start procedure is restricted.
        player: Viewer,
    },
    /// Attacks are forbidden against every target, or only the listed leaders.
    AttackRestriction {
        /// Absent means all targets.
        leaders: Option<Vec<Viewer>>,
    },
    /// Activated abilities are forbidden, with an optional evolution exception.
    AbilityRestriction {
        /// Intrinsic evolution remains permitted.
        except_evolution: bool,
    },
    /// A Drive stand prevents attacks against leaders for this turn.
    NoLeaderAttack,
    /// A frozen damage replacement; lists include only authorized current links.
    Prevention {
        /// Damage category covered by the replacement.
        kind: DamageKind,
        /// Prevent or adjust the damage.
        change: PreventionChange,
        /// Remaining uses, absent for unlimited usage.
        remaining: Option<u64>,
        /// Authorized recipient links, absent for any recipient restriction.
        recipients: Option<Vec<EffectRecipient>>,
        /// Authorized source links, absent for any source restriction.
        sources: Option<Vec<ObjectHandle>>,
    },
    /// Player cost layer; applicability is determined by the contextual quote.
    PlayerCost {
        /// Frozen operation.
        change: NumberChange,
        /// Whether the effect is consumed by one payment.
        limited: bool,
    },
    /// The player can play their own banished cards this turn.
    BanishPlay,
    /// A subscription awaiting its event, without its captures or body.
    Delayed {
        /// Subscribed event kind.
        timing: TriggerTiming,
    },
}

/// One installed change on the observed card; not an authority effect identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledEffect {
    /// Origin authorized at installation; absent for a concealed source.
    pub source: Option<EffectSource>,
    /// Principal expiry.
    pub expires: EffectExpiry,
    /// Whether the principal lifetime is still live.
    pub main_live: bool,
    /// Unfinished repeated intervals.
    pub intervals: Vec<EffectInterval>,
    /// Whether this layer currently contributes, including suppression and interval state.
    pub active: bool,
    /// Frozen player-visible operation.
    pub change: InstalledChange,
}
