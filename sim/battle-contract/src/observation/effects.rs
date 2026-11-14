//! Installed effects, ability budgets and pending occurrences as closed descriptions.
//!
//! Nothing here is executable or names an authority identity.

use crate::common::{ObjectHandle, Seat, SourceRef};
use crate::number::{Int, Nat};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Category of an ability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AbilityKind {
    /// Spell text.
    Spell,
    /// Activated ability.
    Activated,
    /// Triggered ability.
    Triggered,
    /// Static ability.
    Static,
}

/// Which cost a cost layer changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CostKind {
    /// Play cost.
    Play,
    /// Evolution cost.
    Evolution,
}

/// Damage category covered by a prevention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DamageKind {
    /// Any damage.
    Any,
    /// Combat damage.
    Combat,
    /// Ability damage.
    Ability,
}

/// Rules stage that consumes one use of an ability budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsumptionPoint {
    /// When the occurrence is admitted as pending.
    PendingAdmission,
    /// When the play commits.
    PlayCommitted,
    /// At a point defined by the rules.
    RuleDefined,
}

/// Completed event category that a trigger or subscription waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriggerTiming {
    /// A card enters the field.
    EnterField,
    /// Targets are declared.
    Targeted,
    /// An ability resolves.
    Resolve,
    /// A payment commits.
    Payment,
    /// Counters change.
    CounterChange,
    /// A card enters EX.
    EnterEx,
    /// A card leaves the field.
    LeaveField,
    /// A follower attacks.
    Attack,
    /// A follower evolves.
    Evolve,
    /// A phase begins.
    Phase,
    /// A stat changes.
    StatChange,
    /// Posture changes.
    Posture,
    /// A card is destroyed.
    Destroy,
    /// Union Burst is activated.
    UnionBurst,
    /// Cards are fused.
    Fusion,
    /// A card is played.
    Play,
    /// Cards are discarded.
    Discard,
    /// Cards are banished.
    Banish,
    /// Cards are drawn.
    Draw,
    /// Drive is acquired.
    DriveGained,
    /// A Drive check completes.
    DriveChecked,
    /// A Drive trigger completes.
    DriveTrigger,
    /// A die roll completes.
    Dice,
    /// Equipment is attached.
    Equip,
    /// A card moves.
    Move,
    /// A race completes.
    Race,
    /// Leader life changes.
    LifeChange,
    /// Damage is dealt.
    Damage,
}

/// When an installed effect ends, without a private clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectExpiry {
    /// No temporal expiry; ordinary zone rules still apply.
    NoTemporalExpiry,
    /// End of the current resolution.
    ResolutionEnd,
    /// End of a turn that has already begun.
    TurnEnd,
    /// End of a player's next turn, which has not begun.
    NextTurnEnd {
        /// Player whose next turn ends the effect.
        player: Seat,
    },
    /// When the identified source leaves the field.
    SourceLeavesField,
    /// End of a player's next start procedure.
    NextStartEnd {
        /// Player whose start procedure ends the effect.
        player: Seat,
    },
}

/// A repeated active interval, separate from the principal lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectInterval {
    /// Waiting for a player's next turn.
    NextTurn {
        /// Player whose next turn starts the interval.
        player: Seat,
    },
    /// Active during the current turn.
    CurrentTurn,
}

/// A frozen numeric operation, before other layers apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NumberChange {
    /// Add a signed amount.
    Add {
        /// Signed amount.
        amount: Int,
    },
    /// Set a value.
    Set {
        /// New value.
        amount: Int,
    },
}

/// What a damage prevention does to the covered damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreventionChange {
    /// Prevent the damage.
    Prevent,
    /// Add a signed amount to the damage.
    Add {
        /// Signed amount.
        amount: Int,
    },
}

/// A currently identifiable recipient of an effect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectRecipient {
    /// A leader.
    Leader {
        /// The leader's seat.
        player: Seat,
    },
    /// An identifiable card.
    Card {
        /// Key of the card in the object table.
        object: ObjectHandle,
    },
}

/// Public role of a granted ability, without its program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrantedDefinition {
    /// Ability category.
    pub ability: AbilityKind,
    /// Printed line of the definition.
    pub line: Nat,
}

/// One supported installed change; effective values stay in the card state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstalledChange {
    /// Frozen attack and maximum-life layers.
    Numeric {
        /// Attack operation, if any.
        attack: Option<NumberChange>,
        /// Maximum-life operation, if any.
        life: Option<NumberChange>,
    },
    /// A cost layer; the contextual quote is separate.
    Cost {
        /// Cost it changes.
        cost: CostKind,
        /// Frozen operation.
        change: NumberChange,
    },
    /// Keyword, trait and ability grants.
    Grant {
        /// Granted keywords.
        keywords: Vec<String>,
        /// Granted traits.
        traits: Vec<String>,
        /// Granted ability roles.
        definitions: Vec<GrantedDefinition>,
    },
    /// Earlier abilities were removed; later grants can still apply.
    AbilityRemoval {
        /// Whether Box also restricts posture and combat.
        boxed: bool,
    },
    /// An amulet temporarily acts as a follower.
    Pilot,
    /// The next start procedure cannot stand this card.
    NoStand {
        /// Player whose start procedure is restricted.
        player: Seat,
    },
    /// Attacks are forbidden against every target or only against listed leaders.
    AttackRestriction {
        /// Restricted leaders; absent means every target.
        leaders: Option<Vec<Seat>>,
    },
    /// Activated abilities are forbidden.
    AbilityRestriction {
        /// Whether intrinsic evolution stays permitted.
        except_evolution: bool,
    },
    /// A Drive stand forbids attacks against leaders this turn.
    NoLeaderAttack,
    /// A frozen damage replacement.
    Prevention {
        /// Damage category covered.
        damage: DamageKind,
        /// What happens to that damage.
        change: PreventionChange,
        /// Remaining uses; absent means unlimited.
        remaining: Option<Nat>,
        /// Authorized recipient links; absent means any recipient.
        recipients: Option<Vec<EffectRecipient>>,
        /// Authorized source links; absent means any source.
        sources: Option<Vec<ObjectHandle>>,
    },
    /// A player cost layer; the contextual quote decides whether it applies.
    PlayerCost {
        /// Frozen operation.
        change: NumberChange,
        /// Whether one payment consumes the effect.
        limited: bool,
    },
    /// The player may play their own banished cards this turn.
    BanishPlay,
    /// A subscription awaiting its event, without captures or body.
    Delayed {
        /// Awaited event category.
        timing: TriggerTiming,
    },
}

/// One installed change on a card or player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstalledEffect {
    /// Origin authorized at installation; absent for a concealed source.
    pub source: Option<SourceRef>,
    /// Principal expiry.
    pub expires: EffectExpiry,
    /// Whether the principal lifetime is still live.
    pub main_live: bool,
    /// Unfinished repeated intervals.
    pub intervals: Vec<EffectInterval>,
    /// Whether the change currently contributes.
    pub active: bool,
    /// The change itself.
    pub change: InstalledChange,
}

/// Current budget of one ability, without an instance identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AbilityQuota {
    /// Authorized origin of the definition.
    pub source: SourceRef,
    /// Ability category.
    pub ability: AbilityKind,
    /// Limit per turn.
    pub maximum: Nat,
    /// Remaining uses.
    pub remaining: Nat,
    /// Rules stage that consumes a use.
    pub consume_at: ConsumptionPoint,
}

/// One disclosed pending occurrence; never an answerable option.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PendingTrigger {
    /// Authorized origin of the occurrence.
    pub source: SourceRef,
    /// Controller of the occurrence.
    pub controller: Seat,
    /// Completed event category that produced it.
    pub timing: TriggerTiming,
    /// Selections left in its admission group; only its controller receives this.
    pub admission_remaining: Option<Nat>,
}
