use crate::ir::ZoneKind;
use alloc::collections::BTreeMap;
use core::slice::from_ref;
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Document {
    #[serde(rename = "format")]
    pub _format: u8,
    #[serde(rename = "kind")]
    pub _kind: String,
    pub scope: String,
    pub cards: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Card {
    #[serde(rename = "token")]
    pub _token: Option<IgnoredAny>,
    #[serde(rename = "meta")]
    pub _meta: IgnoredAny,
    #[serde(rename = "note")]
    pub _note: Option<IgnoredAny>,
    #[serde(default)]
    pub abilities: Vec<Value>,
    #[serde(default)]
    pub faces: BTreeMap<String, Face>,
    #[serde(default)]
    pub ignored: Vec<DirectSourceUse>,
    #[serde(default)]
    pub token_sources: Vec<TokenSource>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TokenSource {
    pub source: DirectSourceUse,
    #[serde(rename = "card_no")]
    pub _card_no: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Face {
    pub abilities: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ability {
    pub line: u32,
    pub section: Option<u64>,
    pub sources: [SourceUse; 1],
    #[serde(rename = "ports")]
    pub _ports: Ports,
    pub imports: Option<IgnoredAny>,
    pub fanfare: Option<Body>,
    pub lastword: Option<Body>,
    pub attack_time: Option<Body>,
    pub auto: Option<Auto>,
    pub evolve_time: Option<Body>,
    pub super_evolve_time: Option<Body>,
    pub evolve: Option<Evolution>,
    pub feed: Option<Feed>,
    pub race_time: Option<Body>,
    pub activate: Option<Activate>,
    pub spell: Option<Body>,
    pub play: Option<PlayCosts>,
    pub keywords: Option<Vec<String>>,
    pub r#static: Option<Static>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum SourceUse {
    Direct(DirectSourceUse),
    Ability(AbilitySourceUse),
    CardKeyword(CardKeywordSourceUse),
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DirectSourceUse {
    pub admission: String,
    pub owner: Value,
    pub face_id: String,
    pub field: String,
    pub ordinal: Option<u64>,
    pub line: u32,
    pub scope: String,
    pub covers: Vec<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AbilitySourceUse {
    pub binding_id: String,
    pub occurrence: Value,
    pub ability_scope: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CardKeywordSourceUse {
    pub binding_id: String,
    pub occurrence: Value,
}

impl SourceUse {
    pub(super) fn json(&self) -> Value {
        match self {
            Self::Direct(source) => serde_json::json!({
                "admission": source.admission, "owner": source.owner,
                "face_id": source.face_id, "field": source.field,
                "ordinal": source.ordinal, "line": source.line,
                "scope": source.scope, "covers": source.covers,
            }),
            Self::Ability(source) => serde_json::json!({
                "binding_id": source.binding_id,
                "occurrence": source.occurrence,
                "ability_scope": source.ability_scope,
            }),
            Self::CardKeyword(source) => serde_json::json!({
                "binding_id": source.binding_id,
                "occurrence": source.occurrence,
            }),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Activate {
    pub quick: Option<bool>,
    pub zone: Option<ActivationZones>,
    pub cost: Cost,
    pub usable_if: Option<Condition>,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ActivationZones {
    One(ActivationZone),
    Many(Vec<ActivationZone>),
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ActivationZone {
    Hand,
    Cemetery,
    Field,
    Ex,
}

impl ActivationZone {
    pub(super) const fn kind(self) -> ZoneKind {
        match self {
            Self::Hand => ZoneKind::Hand,
            Self::Cemetery => ZoneKind::Cemetery,
            Self::Field => ZoneKind::Field,
            Self::Ex => ZoneKind::Ex,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Cost {
    Short(Box<CostOperation>),
    Ordered { steps: Vec<CostOperation> },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CostOperation {
    Pp(u64),
    Lesson(u64),
    Counter(CounterCost),
    LeaderLife(u64),
    ToCemetery(Material),
    ReturnToHand(Material),
    ToEx(Material),
    Banish(Material),
    Act(Material),
    Discard(DiscardCost),
    BanishSelf(bool),
    ActSelf(bool),
    SelfToCemetery(bool),
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum DiscardCost {
    Material(Box<HandMaterial>),
    Reference { r#ref: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ports {
    #[serde(rename = "imports")]
    pub _imports: Empty,
    #[serde(rename = "exports")]
    pub _exports: Empty,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Body {
    pub ub: Option<True>,
    #[serde(rename = "cost")]
    pub head_cost: Option<Cost>,
    pub optional_cost: Option<BodyOptionalCost>,
    pub per_turn: Option<u64>,
    #[serde(default)]
    pub targets: Vec<Target>,
    #[serde(default)]
    pub define: BTreeMap<String, Number>,
    #[serde(rename = "do")]
    pub steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Step {
    Pay(ModePayment),
    Counter(CounterChange),
    Optional(Optional),
    Race(Race),
    Evolve(EffectEvolution),
    If(Conditional),
    EachPlayer(EachPlayer),
    Choice(Choice),
    Either(Either),
    Choose(Choose),
    Draw(Draw),
    Pp(ResourceIncrease),
    Discard(Discard),
    PpMax(ResourceIncrease),
    Ep(ResourceIncrease),
    Damage(Damage),
    Life(Life),
    SetLife(SetLife),
    Destroy(ObjectAction),
    Banish(BanishAction),
    Transform(Transform),
    Act(ObjectAction),
    Stand(ObjectAction),
    Buff(Modify),
    Grant(Grant),
    SetStat(Modify),
    Apply(Apply),
    PlayerEffect(PlayerEffect),
    Search(Search),
    Look(Look),
    Reveal(Reveal),
    Move(Move),
    Create(Create),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Transform {
    #[serde(rename = "ref")]
    pub reference: Objects,
    pub into: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CounterCost {
    pub on: CounterCostHolder,
    pub holders: Option<u64>,
    pub name: String,
    pub n: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum CounterCostHolder {
    This(This),
    Selected(Box<PublicQuery>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CounterChange {
    pub on: Objects,
    pub name: String,
    pub delta: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CounterRead {
    pub on: This,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModePayment {
    pub cost: Cost,
    #[serde(rename = "do")]
    pub steps: Vec<Step>,
    #[serde(rename = "who")]
    pub _who: Option<SelfPlayer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Optional {
    #[serde(rename = "do")]
    pub steps: Vec<Step>,
    #[serde(rename = "who")]
    pub _who: Option<SelfPlayer>,
    pub then: Option<Vec<Step>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Feed {
    pub times: Option<u64>,
    pub cost: EvolutionCost,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Race {
    #[serde(rename = "ref")]
    pub _reference: This,
    pub n: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectEvolution {
    #[serde(rename = "ref")]
    pub _reference: This,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Create {
    pub token: CreateToken,
    pub to: CreateZone,
    pub n: Option<TokenCount>,
    pub result: Option<String>,
    pub bind: Option<String>,
    pub by: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum CreateToken {
    One(String),
    Many(Vec<String>),
}

impl CreateToken {
    pub(super) fn names(&self) -> &[String] {
        match self {
            Self::One(name) => from_ref(name),
            Self::Many(names) => names,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EachPlayer {
    #[serde(rename = "as")]
    pub binding: String,
    pub stages: Vec<Vec<Step>>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum TokenCount {
    Fixed(u64),
    Receipt { count: TokenCollection },
    Query { count: CountQuery },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TokenCollection {
    pub r#in: TokenBefore,
    #[serde(rename = "as")]
    pub binding: String,
    pub r#where: TokenPredicate,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TokenBefore {
    pub objects_of: String,
    pub view: BeforeOnly,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BeforeOnly {
    Before,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TokenPredicate {
    #[serde(rename = "cmp")]
    pub player_cmp: TokenPlayerComparison,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TokenPlayerComparison {
    pub left: TokenMaster,
    pub op: EqualOnly,
    pub right: TokenPlayer,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EqualOnly {
    Eq,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TokenMaster {
    pub player: TokenMasterOf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TokenMasterOf {
    pub master_of: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TokenPlayer {
    pub player: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CreateZone {
    Ex,
    Field,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Life {
    pub who: SelfPlayer,
    pub delta: Number,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceIncrease {
    pub who: SelfPlayer,
    pub delta: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Discard {
    pub n: Option<u64>,
    pub all: Option<bool>,
    pub who: Option<LeaderSide>,
    pub random: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SetLife {
    pub who: SelfPlayer,
    pub n: Number,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Look {
    pub top: DeckTop,
    pub bind: String,
    #[serde(rename = "by")]
    pub _by: Option<SelfPlayer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeckTop {
    pub who: SelfPlayer,
    pub n: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Move {
    pub r#ref: Option<MoveObjects>,
    pub top: Option<DeckTop>,
    pub to: Destination,
    pub position: Option<Placement>,
    pub order: Option<Chooser>,
    pub result: Option<String>,
    #[serde(rename = "by")]
    pub _by: Option<SelfPlayer>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum MoveObjects {
    Observed(Observed),
    Field(Objects),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Observed {
    pub r#in: Observation,
    pub except: Option<Box<Self>>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Observation {
    Binding(String),
    Before { objects_of: String, view: Before },
}

#[derive(Deserialize)]
pub(super) enum Before {
    #[serde(rename = "before")]
    Before,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Destination {
    Field,
    Deck,
    Ex,
    Hand,
    Cemetery,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
#[expect(
    variant_size_differences,
    reason = "A fixed index and an edge string keep the authored placement grammar unboxed."
)]
pub(super) enum Placement {
    Edge(DeckEdge),
    Nth { nth: u64 },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DeckEdge {
    Top,
    Bottom,
}

#[derive(Deserialize)]
pub(super) enum Chooser {
    #[serde(rename = "chooser")]
    Chooser,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum BanishAction {
    Top { top: DeckTop },
    Objects(Box<ObjectAction>),
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum ObjectAction {
    Short(Objects),
    Long {
        r#ref: Objects,
        result: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Modify {
    pub on: Objects,
    pub power: Option<Number>,
    pub hp: Option<Number>,
    pub until: Option<Until>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Until {
    Turn,
    NextOppTurn,
    ThisAndNextOppTurn,
    Game,
    #[serde(untagged)]
    DuringNextTurn {
        during_next_turn: AttackTurnInterval,
    },
    #[serde(untagged)]
    NextPhase {
        next_phase: PhaseEdge,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PhaseEdge {
    pub who: StandPlayer,
    pub phase: StartPhase,
    pub edge: EndEdge,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StartPhase {
    Start,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EndEdge {
    End,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
pub(super) enum StandPlayer {
    #[serde(rename = "self")]
    Controller,
    #[serde(rename = "opp")]
    Opponent,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Objects {
    This(This),
    Bound(String),
    Zone(ObjectQuery),
    Follow { follow: String },
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObjectQuery {
    #[serde(rename = "as")]
    pub alias: Option<String>,
    pub r#where: Option<Box<QueryExtremumWhere>>,
    pub who: Side,
    pub zone: Zone,
    pub kind: Option<Follower>,
    pub r#trait: Option<String>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub token: Option<bool>,
    pub not_token: Option<bool>,
    pub orig_cost_le: Option<u64>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    #[serde(default)]
    pub other: bool,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub(super) enum Side {
    #[serde(rename = "self")]
    Controller,
    #[serde(rename = "opp")]
    Opponent,
    #[serde(rename = "all")]
    All,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Zone {
    Field,
    Ex,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Apply {
    pub on: Objects,
    pub effect: InstalledEffect,
    pub until: Option<Until>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum InstalledEffect {
    Stat(ConstantStats),
    CostMod(CostModifier),
    EvolveCostMod(EvolutionCostModifier),
    Cannot(Cannot),
    Prevent(Prevent),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CannotStand {
    pub what: StandAction,
    pub on: Affected,
    pub during: StandDuring,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StandAction {
    Stand,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StandDuring {
    pub phase: StartPhase,
    pub who: StandPlayer,
    pub cause: RuleCause,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RuleCause {
    Rule,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvolutionCostModifier {
    pub on: Affected,
    pub delta: Option<i64>,
    pub set: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CostModifier {
    pub when_playing: Playing,
    pub delta: Option<i64>,
    pub set: Option<i64>,
    pub next_only: Option<bool>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Playing {
    Affected(AffectedCondition),
    Union { any_of: Vec<PlayingQuery> },
    Query(PlayingQuery),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayingQuery {
    pub who: Side,
    pub zone: Option<PlayingZones>,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    #[serde(default)]
    pub traits_all: Vec<String>,
    pub class: Option<String>,
    pub name: Option<String>,
    pub orig_cost_le: Option<u64>,
    pub token: Option<bool>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum PlayingZones {
    One(PlayingZone),
    Many(Vec<PlayingZone>),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PlayingZone {
    Hand,
    Ex,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerEffect {
    pub who: Side,
    pub effect: InstalledEffect,
    pub until: Option<Until>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AffectedCondition {
    #[serde(rename = "in")]
    pub subjects: Affected,
}

#[derive(Deserialize)]
pub(super) enum Affected {
    #[serde(rename = "affected")]
    Affected,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Search {
    pub sel: Option<SearchSelector>,
    pub n: Option<u64>,
    pub upto: Option<u64>,
    pub groups: Option<Vec<SearchGroup>>,
    pub to: SearchDestination,
    pub result: Option<String>,
    pub distinct_by: Option<Names>,
}

#[derive(Clone, Deserialize)]
pub(super) enum Names {
    #[serde(rename = "name")]
    Name,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchGroup {
    pub sel: SearchSelector,
    pub n: Option<u64>,
    pub upto: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum SearchSelector {
    Query(Box<SearchQuery>),
    Union { any_of: Vec<SearchQuery> },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchQuery {
    pub who: SelfPlayer,
    pub zone: DeckZone,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    #[serde(default)]
    pub traits_all: Vec<String>,
    pub name: Option<String>,
    pub class: Option<String>,
    pub name_contains: Option<String>,
    #[serde(default)]
    pub not_same_name_as_this: bool,
    pub keyword: Option<String>,
    #[serde(rename = "as")]
    pub alias: Option<String>,
    pub r#where: Option<SearchWhere>,
    pub cost_eq: Option<u64>,
    pub cost_le: Option<u64>,
    pub cost_ge: Option<u64>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_le: Option<u64>,
    pub orig_cost_ge: Option<u64>,
    pub has_evolve: Option<True>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchWhere {
    pub cmp: SearchPowerComparison,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchPowerComparison {
    pub left: SearchPower,
    pub op: Comparison,
    pub right: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchPower {
    pub power_of: String,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum SearchKind {
    EvolveFollower,
    Follower,
    Spell,
    Amulet,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum SearchDestination {
    Single(SearchZone),
    Partition {
        order: Printed,
        partition: Vec<SearchPart>,
    },
    Replacement {
        zone: SearchZone,
        instead: SearchReplacement,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SearchZone {
    Hand,
    Cemetery,
    Ex,
    Field,
}

#[derive(Deserialize)]
pub(super) enum Printed {
    #[serde(rename = "printed")]
    Printed,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchPart {
    pub zone: SearchZone,
    pub n: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchReplacement {
    pub r#if: FoundCondition,
    pub zone: SearchZone,
    pub may: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FoundCondition {
    pub cmp: FoundComparison,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FoundComparison {
    pub left: FoundCost,
    pub op: LessEqual,
    pub right: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FoundCost {
    pub cost_of: Found,
}

#[derive(Deserialize)]
pub(super) enum Found {
    #[serde(rename = "found")]
    Found,
}

#[derive(Deserialize)]
pub(super) enum LessEqual {
    #[serde(rename = "le")]
    LessEqual,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConstantStats {
    pub power: Option<i64>,
    pub hp: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Draw {
    pub n: Number,
    #[serde(rename = "who")]
    pub _who: Option<SelfPlayer>,
}

#[derive(Deserialize)]
pub(super) enum SelfPlayer {
    #[serde(rename = "self")]
    Controller,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Damage {
    pub instead: Option<Vec<DamageInstead>>,
    pub to: Recipients,
    pub n: Number,
    pub distribute: Option<bool>,
    pub distribute_at: Option<AtPlay>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamageInstead {
    pub r#if: Condition,
    pub n: Number,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Recipients {
    Union { any_of: Vec<Self> },
    Field(ObjectQuery),
    Leader { leader: LeaderSide },
    LeaderOf { leader_of: String },
    Leaders { leaders: LeaderGroup },
    Target(String),
}

#[derive(Deserialize)]
pub(super) enum AtPlay {
    #[serde(rename = "play")]
    Play,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    #[serde(default)]
    pub any: bool,
    pub option: Option<u64>,
    pub bind: String,
    pub from: TargetQuery,
    pub distinct_by: Option<Names>,
    pub n: Option<u64>,
    pub upto: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Choose {
    #[serde(default)]
    pub any: bool,
    pub bind: String,
    pub from: TargetQuery,
    pub groups: Option<Vec<ChooseGroup>>,
    pub distinct_by: Option<Names>,
    pub n: Option<u64>,
    pub upto: Option<u64>,
    pub may: Option<bool>,
    pub reveal: Option<bool>,
    pub order: Option<bool>,
    pub by: Option<LeaderSide>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChooseGroup {
    pub kind: Option<SearchKind>,
    pub orig_cost_le: Option<u64>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetQuery {
    #[serde(rename = "as")]
    pub alias: Option<String>,
    pub r#where: Option<Box<QueryExtremumWhere>>,
    #[serde(rename = "in")]
    pub observation: Option<String>,
    pub who: Option<Side>,
    pub zone: Option<TargetZone>,
    pub leader: Option<LeaderSide>,
    pub leaders: Option<LeaderGroup>,
    #[serde(default)]
    pub leader_or: bool,
    pub kind: Option<SearchKind>,
    pub keyword: Option<FilterKeyword>,
    pub r#trait: Option<String>,
    #[serde(default)]
    pub traits_all: Vec<String>,
    pub token: Option<bool>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub names: Vec<String>,
    pub name_contains: Option<String>,
    #[serde(default)]
    pub other: bool,
    #[serde(default)]
    pub not_same_name_as_this: bool,
    pub cost_eq: Option<u64>,
    pub cost_le: Option<u64>,
    pub cost_ge: Option<u64>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_le: Option<u64>,
    pub orig_cost_ge: Option<u64>,
    pub hp_le: Option<u64>,
    pub has_evolve: Option<True>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String")]
pub(super) struct FilterKeyword(pub super::super::KeywordId);

impl TryFrom<String> for FilterKeyword {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match super::keywords::keyword_id(&value) {
            Some(
                keyword @ (super::super::KeywordId::Ward
                | super::super::KeywordId::Aura
                | super::super::KeywordId::Storm
                | super::super::KeywordId::Rush
                | super::super::KeywordId::Bane
                | super::super::KeywordId::Intimidate
                | super::super::KeywordId::Assail),
            ) => Ok(Self(keyword)),
            _ => Err("unsupported filter keyword"),
        }
    }
}

impl TargetQuery {
    pub(super) fn has_numeric_filter(&self) -> bool {
        self.alias.is_some()
            || self.r#where.is_some()
            || [
                self.cost_eq,
                self.cost_le,
                self.cost_ge,
                self.orig_cost_eq,
                self.orig_cost_le,
                self.orig_cost_ge,
                self.hp_le,
            ]
            .into_iter()
            .any(|limit| limit.is_some())
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TargetZone {
    Field,
    Ex,
    Cemetery,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Choice {
    pub n: Option<u64>,
    pub upto: Option<u64>,
    pub instead: Option<Vec<ChoiceInstead>>,
    pub exclude_chosen: Option<ThisTurn>,
    pub options: Vec<Vec<Step>>,
    #[serde(rename = "who")]
    pub _who: Option<SelfPlayer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Either {
    pub options: Vec<Vec<Step>>,
    #[serde(rename = "who")]
    pub _who: Option<SelfPlayer>,
    pub bind: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LeaderGroup {
    All,
    Opponents,
}

#[derive(Clone, Copy, Deserialize)]
pub(super) enum LeaderSide {
    #[serde(rename = "self")]
    Controller,
    #[serde(rename = "opp")]
    Opponent,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Number {
    Literal(i64),
    Counter {
        counter: CounterRead,
    },
    Alias(String),
    Power {
        power_of: This,
    },
    Life {
        hp_of: BodyStatReference,
    },
    Cost {
        cost_of: This,
    },
    OriginalCost {
        orig_cost_of: This,
    },
    HistoryCount {
        history_count: HistoryQuery,
    },
    LeaderLife {
        leader_life: LeaderSide,
    },
    Minus {
        minus: Box<[Self; 2]>,
    },
    Count {
        count: CountQuery,
    },
    ReceiptCount {
        count: ReceiptCountQuery,
    },
    CountOf {
        count_of: String,
    },
    Aggregate {
        agg: Aggregate,
    },
    DistinctNames {
        distinct_names: PublicQuery,
    },
    DistinctOriginalCosts {
        distinct_orig_costs: PublicQuery,
    },
    Resource {
        resource: ResourceQuery,
    },
    TurnOrdinal {
        turn_number: TurnNumberQuery,
    },
    Resolution {
        at: ResolutionTime,
        value: Box<Self>,
    },
    Selection {
        at: SelectionTime,
        selection: String,
        value: SelectedStat,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryQuery {
    #[serde(rename = "event")]
    pub event: HistoryEvent,
    #[serde(rename = "window")]
    pub _window: ThisTurn,
    pub who: Option<LeaderSide>,
    #[serde(rename = "distinct")]
    pub distinct: Option<HistoryDistinct>,
    pub sel: Option<MovementSubject>,
    pub from: Option<HistoryOrigin>,
    pub to: Option<DepartureDestination>,
    pub at_least: Option<u64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HistoryEvent {
    Played,
    Attacked,
    Evolved,
    Destroyed,
    LeftField,
    Moved,
}
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum HistoryOrigin {
    Zone(FieldOrigin),
    Owned { who: LeaderSide, zone: FieldOrigin },
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HistoryDistinct {
    Facts,
    Subjects,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ResolutionTime {
    Resolution,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SelectionTime {
    Selection,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SelectedStat {
    #[serde(rename = "hp_of")]
    Hp(String),
    #[serde(rename = "power_of")]
    Power(String),
    #[serde(rename = "cost_of")]
    Cost(String),
    #[serde(rename = "orig_cost_of")]
    OriginalCost(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CountQuery {
    pub face_up: Option<bool>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    #[serde(default)]
    pub other: bool,
    pub who: Side,
    pub zone: CountZone,
    pub kind: Option<SearchKind>,
    pub keyword: Option<FilterKeyword>,
    pub r#trait: Option<String>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub orig_cost_eq: Option<u64>,
    pub token: Option<bool>,
    pub not_token: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CountZone {
    EvolveDeck,
    Field,
    Ex,
    Deck,
    Hand,
    Cemetery,
    Banish,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceQuery {
    pub who: LeaderSide,
    pub kind: PpResource,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PpResource {
    Pp,
    PpMax,
    Ep,
    Sep,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TurnNumberQuery {
    pub who: LeaderSide,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DeckZone {
    Deck,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "An empty JSON object is required here; a unit struct accepts null."
)]
pub(super) struct Empty {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Auto {
    pub on: PhaseSubscription,
    pub zone: Option<ActivationZones>,
    #[serde(flatten)]
    pub body: Body,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum PhaseSubscription {
    #[serde(rename = "evolved")]
    Evolved(EvolvedSubscription),
    #[serde(rename = "played")]
    Played(PlayedSubscription),
    #[serde(rename = "phase")]
    Phase(PhaseSubscriptionPayload),
    #[serde(rename = "enter_field")]
    EnterField(EntrySubscription),
    #[serde(rename = "left_field")]
    LeaveField(LeaveSubscription),
    #[serde(rename = "destroyed")]
    Destroyed(LeaveSubscription),
    #[serde(rename = "moved")]
    Moved(MoveSubscription),
    #[serde(rename = "placed_ex")]
    PlacedEx(ExSubscription),
    #[serde(rename = "damaged")]
    Damage(DamageSubscription),
    #[serde(rename = "attacked")]
    Attacked(AttackSubscription),
    #[serde(rename = "dealt_damage")]
    Dealt(DealtSubscription),
    #[serde(rename = "life_changed")]
    LifeChanged(LifeSubscription),
    #[serde(rename = "acted")]
    Acted(PostureSubscription),
    #[serde(rename = "stood")]
    Stood(PostureSubscription),
    #[serde(rename = "stat_changed")]
    StatChanged(StatSubscription),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PhaseSubscriptionPayload {
    pub which: SubscribedPhase,
    pub whose: PhasePlayer,
}
#[derive(Deserialize)]
pub(super) enum SubscribedPhase {
    #[serde(rename = "main")]
    Main,
    #[serde(rename = "end")]
    End,
}
#[derive(Deserialize)]
pub(super) enum PhasePlayer {
    #[serde(rename = "self")]
    Controller,
    #[serde(rename = "opp")]
    Opponent,
    #[serde(rename = "each")]
    Each,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evolution {
    pub cost: Cost,
    pub usable_if: Option<Condition>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvolutionCost {
    pub pp: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EntrySubscription {
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub who: Option<EntryPlayer>,
    pub sel: Option<MovementSubject>,
    pub during: Option<During>,
}
#[derive(Deserialize)]
pub(super) enum This {
    #[serde(rename = "this")]
    This,
}
#[derive(Deserialize)]
pub(super) enum EntryPlayer {
    #[serde(rename = "self")]
    Controller,
    #[serde(rename = "opp")]
    Opponent,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct During {
    pub turn: SelfPlayer,
}
#[derive(Clone, Deserialize)]
pub(super) enum Follower {
    #[serde(rename = "follower")]
    Follower,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayCosts {
    #[serde(default)]
    pub define: BTreeMap<String, Number>,
    pub cannot_if: Option<PlayRestriction>,
    pub cost_delta: Option<Value>,
    pub cost_set: Option<Value>,
    pub optional_cost: Option<OptionalCost>,
    pub optional_cost_delta: Option<i64>,
    pub optional_cost_set: Option<i64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum PlayRestriction {
    Named(NamedPlayRestriction),
    Condition(Condition),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NamedPlayRestriction {
    MyTurn(bool),
    InZone(PlayRestrictionZone),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayRestrictionZone {
    pub r#ref: String,
    pub zone: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CostCondition {
    Spellchain(u64),
    Necro(u64),
    Awakened(bool),
    Crimson(bool),
    LeaderLife(PlayerThreshold),
    InZone(PlayRestrictionZone),
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
    Cmp(CostComparison),
    Exists(PublicQuery),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CostComparison {
    pub left: PublicNumber,
    pub op: Comparison,
    pub right: i64,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Comparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum PublicNumber {
    Literal(i64),
    Count { count: PublicQuery },
    DistinctNames { distinct_names: PublicQuery },
    DistinctOriginalCosts { distinct_orig_costs: PublicQuery },
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicQuery {
    pub racing: Option<bool>,
    #[serde(default)]
    pub other: bool,
    pub who: Side,
    pub zone: PublicZone,
    pub kind: Option<SearchKind>,
    pub keyword: Option<FilterKeyword>,
    pub r#trait: Option<String>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub token: Option<bool>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_le: Option<u64>,
    pub orig_cost_ge: Option<u64>,
    pub cost_eq: Option<u64>,
    pub cost_le: Option<u64>,
    pub cost_ge: Option<u64>,
    pub hp_le: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ExistsQuery {
    Public(PublicQuery),
    Selected(SelectedQuery),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SelectedQuery {
    #[serde(rename = "in")]
    pub reference: String,
    pub kind: Option<SearchKind>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub r#trait: Option<String>,
    #[serde(default)]
    pub traits_all: Vec<String>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub acted: Option<bool>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum PublicZone {
    Field,
    Ex,
    Cemetery,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConditionalChange {
    pub r#if: CostCondition,
    pub delta: Option<Number>,
    pub set: Option<Number>,
}

#[derive(Deserialize)]
pub(super) enum ThisTurn {
    #[serde(rename = "this_turn")]
    ThisTurn,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChoiceInstead {
    pub r#if: ModeCondition,
    pub n: Option<u64>,
    pub upto: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ModeCondition {
    Spellchain(u64),
    Necro(u64),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OptionalCost {
    pub to_cemetery: Option<Material>,
    pub banish: Option<Material>,
    pub act: Option<Material>,
    pub result: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Material {
    pub sel: PublicQuery,
    pub n: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Conditional {
    pub cond: Condition,
    pub then: Vec<Step>,
    pub r#else: Option<Vec<Step>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Condition {
    Is(AttackTargetCondition),
    History(HistoryQuery),
    EnteredFrom(EntryOriginCondition),
    EnteredBy(EntryCondition),
    PlayedFrom(PlayedFromCondition),
    Exists(Box<ExistsQuery>),
    Awakened(bool),
    LeaderLife(PlayerThreshold),

    Crimson(bool),
    Combo(u64),
    Spellchain(u64),
    Necro(u64),
    Did(String),
    Paid(PaidReceipt),
    Cmp(Box<BodyComparison>),
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EntryOriginCondition {
    #[serde(rename = "ref")]
    pub _reference: This,
    #[serde(rename = "from")]
    pub zone: EntryOriginZone,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EntryOriginZone {
    Hand,
    Deck,
    Cemetery,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EntryCondition {
    #[serde(rename = "ref")]
    pub _reference: This,
    pub by: EntryMethod,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EntryMethod {
    Ability,
    Play,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayedFromCondition {
    #[serde(rename = "ref")]
    pub _reference: This,
    pub from: PlayedZone,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PlayedZone {
    Ex,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerThreshold {
    pub who: LeaderSide,
    pub eq: Option<u64>,
    pub le: Option<u64>,
    pub ge: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodyComparison {
    pub left: Number,
    pub op: Comparison,
    pub right: Number,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PaidReceipt {
    pub receipt: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LeaveSubscription {
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub who: Option<EntryPlayer>,
    pub sel: Option<MovementSubject>,
    pub during: Option<During>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamageSubscription {
    #[serde(rename = "recipient")]
    pub recipient: Option<DamageRecipientSubscription>,
    pub kind: Option<AbilityDamage>,
    pub during: Option<DamageDuring>,
}
#[derive(Deserialize)]
pub(super) enum AbilityDamage {
    #[serde(rename = "attack")]
    Attack,
    #[serde(rename = "combat")]
    Combat,
    #[serde(rename = "ability")]
    Ability,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamageDuring {
    pub turn: EntryPlayer,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reveal {
    pub r#ref: RevealRef,
    #[serde(rename = "bind")]
    pub _bind: String,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum RevealRef {
    WholeHand(WholeHand),
    Bound(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WholeHand {
    pub who: Side,
    pub zone: HandZone,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HandZone {
    Hand,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodyOptionalCost {
    #[serde(flatten)]
    pub operation: CostOperation,
    pub result: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HandMaterial {
    pub result: Option<String>,
    pub sel: HandQuery,
    pub n: Option<u64>,
    pub upto: Option<u64>,
    #[serde(default)]
    pub any: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HandQuery {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
    #[serde(rename = "zone")]
    pub _zone: HandZone,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    pub name: Option<String>,
    pub class: Option<String>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_ge: Option<u64>,
    pub orig_cost_le: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Grant {
    pub on: Objects,
    pub keywords: Vec<String>,
    pub until: Option<Until>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayedSubscription {
    pub who: Option<EntryPlayer>,
    #[serde(default)]
    pub sel: Option<MovementSubject>,
    #[serde(default)]
    pub during: Option<During>,
}
impl Auto {
    pub(super) const fn cemetery_play(&self) -> bool {
        matches!(self.on, PhaseSubscription::Played(_))
            && match &self.zone {
                Some(ActivationZones::One(ActivationZone::Cemetery)) => true,
                Some(ActivationZones::Many(zones)) => {
                    matches!(zones.as_slice(), [ActivationZone::Cemetery])
                }
                _ => false,
            }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MoveSubscription {
    pub from: MoveOrigin,
    pub to: DepartureDestination,
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub sel: Option<MovementSubject>,
    pub during: Option<During>,
}
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum MoveOrigin {
    Zone(FieldOrigin),
    Owned {
        who: EntryPlayer,
        #[serde(rename = "zone")]
        _zone: FieldOrigin,
    },
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FieldOrigin {
    Field,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExSubscription {
    pub who: Option<EntryPlayer>,
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub sel: Option<MovementSubject>,
    pub during: Option<During>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MovementSubject {
    pub class: Option<String>,
    pub kind: Option<SearchKind>,
    #[serde(rename = "trait")]
    pub trait_id: Option<String>,
    pub name: Option<String>,
    pub token: Option<bool>,
    pub not_token: Option<bool>,
    pub other: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DepartureDestination {
    Cemetery,
    Hand,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvolvedSubscription {
    pub who: Option<EntryPlayer>,
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub sel: Option<MovementSubject>,
}

#[derive(Clone, Deserialize)]
#[serde(try_from = "bool")]
pub(super) struct True;

impl TryFrom<bool> for True {
    type Error = &'static str;

    fn try_from(value: bool) -> Result<Self, Self::Error> {
        if value {
            Ok(Self)
        } else {
            Err("expected true")
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Static {
    pub effects: Vec<StaticEffect>,
    pub r#while: Option<Condition>,
    pub zone: Option<ActivationZones>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StaticEffect {
    CostMod(CostModifier),
    Grant(StaticGrant),
    Cannot(Cannot),
    Prevent(Prevent),
    DamageMod(DamageModifier),
    DamageCap(DamageCap),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StaticGrant {
    pub on: Option<Objects>,
    pub keywords: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Cannot {
    Stand(CannotStand),
    DealDamage(CannotDealDamage),
    Attack(Box<CannotAttack>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CannotDealDamage {
    #[serde(rename = "what")]
    pub _what: DealDamageAction,
    pub on: Option<Objects>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DealDamageAction {
    DealDamage,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prevent {
    pub on: Option<Recipients>,
    pub kind: PreventionKind,
    pub source: Option<Objects>,
    pub next_only: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PreventionKind {
    Any,
    Combat,
    Ability,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Aggregate {
    pub of: PublicQuery,
    pub attr: AggregateAttribute,
    #[serde(rename = "fn")]
    pub function: AggregateFunction,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AggregateAttribute {
    Cost,
    OrigCost,
    Power,
    Hp,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AggregateFunction {
    Sum,
    Min,
    Max,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QueryExtremumWhere {
    pub cmp: QueryExtremumComparison,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QueryExtremumComparison {
    pub left: QueryStat,
    pub op: EqualOnly,
    pub right: QueryAggregate,
}
#[derive(Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum QueryAggregate {
    Inline { agg: Box<Aggregate> },
    Bound { bound: String },
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum QueryStat {
    #[serde(rename = "cost_of")]
    Cost(String),
    #[serde(rename = "orig_cost_of")]
    OriginalCost(String),
    #[serde(rename = "power_of")]
    Attack(String),
    #[serde(rename = "hp_of")]
    Life(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReceiptCountQuery {
    pub r#in: BeforeObjects,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    pub class: Option<String>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BeforeObjects {
    pub objects_of: String,
    #[serde(rename = "view")]
    pub _view: Before,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamageModifier {
    pub on: Option<Recipients>,
    pub source: Option<Objects>,
    pub kind: Option<PreventionKind>,
    pub delta: i64,
    pub next_only: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamageCap {
    pub on: Option<Recipients>,
    pub threshold: u64,
    pub set: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttackSubscription {
    pub who: Option<EntryPlayer>,
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub sel: Option<AttackSubject>,
    pub during: Option<DamageDuring>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttackSubject {
    pub class: Option<String>,
    pub kind: Option<SearchKind>,
    #[serde(rename = "trait")]
    pub trait_id: Option<String>,
    pub name: Option<String>,
    pub token: Option<bool>,
    pub not_token: Option<bool>,
    pub other: Option<bool>,
    pub keyword: Option<String>,
    #[serde(rename = "as")]
    pub alias: Option<String>,
    pub r#where: Option<AttackCondition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AttackCondition {
    Cmp(SearchPowerComparison),
    Any(Vec<Self>),
    All(Vec<Self>),
    Exists(SubjectKeyword),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SubjectKeyword {
    pub r#in: String,
    pub keyword: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DealtSubscription {
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub who: Option<EntryPlayer>,
    pub sel: Option<MovementSubject>,
    pub recipient: Option<DamageRecipientSubscription>,
    pub kind: Option<AbilityDamage>,
    pub during: Option<DamageDuring>,
}
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum DamageRecipientSubscription {
    Source(This),
    Leader { leader: EntryPlayer },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LifeSubscription {
    pub who: Option<EntryPlayer>,
    pub direction: Option<LifeDirection>,
    pub during: Option<DamageDuring>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LifeDirection {
    Up,
    Down,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CannotAttack {
    pub what: AttackAction,
    pub on: Option<Objects>,
    pub target: Option<Recipients>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AttackAction {
    Attack,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttackTurnInterval {
    pub who: StandPlayer,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PostureSubscription {
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub who: Option<EntryPlayer>,
    pub sel: Option<MovementSubject>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StatSubscription {
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    pub stat: Option<ChangeStats>,
    pub direction: Option<LifeDirection>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ChangeStats {
    One(ChangeStat),
    Many(Vec<ChangeStat>),
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ChangeStat {
    Power,
    Hp,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BodyStatReference {
    This,
    AttackTarget,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttackTargetCondition {
    #[serde(rename = "ref")]
    pub reference: AttackTargetReference,
    pub kind: FollowerKind,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AttackTargetReference {
    AttackTarget,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FollowerKind {
    Follower,
}
