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
    Cemetery,
    Field,
    Ex,
}

impl ActivationZone {
    pub(super) const fn kind(self) -> ZoneKind {
        match self {
            Self::Cemetery => ZoneKind::Cemetery,
            Self::Field => ZoneKind::Field,
            Self::Ex => ZoneKind::Ex,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Cost {
    Short(CostOperation),
    Ordered { steps: Vec<CostOperation> },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CostOperation {
    Pp(u64),
    Lesson(u64),
    LeaderLife(u64),
    ToCemetery(Material),
    ReturnToHand(Material),
    ToEx(Material),
    Banish(Material),
    Act(Material),
    Discard(HandMaterial),
    ActSelf(bool),
    SelfToCemetery(bool),
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
    Act(ObjectAction),
    Stand(ObjectAction),
    Buff(Modify),
    Grant(Grant),
    SetStat(Modify),
    Apply(Apply),
    Search(Search),
    Look(Look),
    Reveal(Reveal),
    Move(Move),
    Create(Create),
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

#[derive(Deserialize)]
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
    #[serde(rename = "who")]
    pub _who: Option<SelfPlayer>,
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
#[serde(rename_all = "snake_case")]
pub(super) enum Placement {
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
    Objects(ObjectAction),
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

#[derive(Clone, Deserialize)]
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
    Cannot(CannotStand),
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
pub(super) struct CostModifier {
    pub when_playing: AffectedCondition,
    pub delta: i64,
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

#[derive(Clone, Deserialize)]
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
    pub bind: String,
    pub from: TargetQuery,
    pub groups: Option<Vec<ChooseGroup>>,
    pub distinct_by: Option<Names>,
    pub n: Option<u64>,
    pub upto: Option<u64>,
    pub may: Option<bool>,
    pub reveal: Option<bool>,
    pub order: Option<bool>,
    #[serde(rename = "by")]
    pub _by: Option<SelfPlayer>,
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
    #[serde(rename = "in")]
    pub observation: Option<String>,
    pub who: Option<Side>,
    pub zone: Option<TargetZone>,
    pub leader: Option<LeaderSide>,
    pub leaders: Option<LeaderGroup>,
    #[serde(default)]
    pub leader_or: bool,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub name: Option<String>,
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

impl TargetQuery {
    pub(super) fn has_numeric_filter(&self) -> bool {
        [
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
    Alias(String),
    HistoryCount {
        history_count: PlayedHistory,
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
    DistinctNames {
        distinct_names: PublicQuery,
    },
    DistinctOriginalCosts {
        distinct_orig_costs: PublicQuery,
    },
    Resource {
        resource: ResourceQuery,
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
pub(super) struct PlayedHistory {
    #[serde(rename = "event")]
    pub _event: PlayedEvent,
    #[serde(rename = "window")]
    pub _window: ThisTurn,
    pub who: Option<LeaderSide>,
    #[serde(rename = "distinct")]
    pub _distinct: Option<FactCount>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PlayedEvent {
    Played,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FactCount {
    Facts,
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
    HpOf(String),
    PowerOf(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CountQuery {
    pub face_up: Option<bool>,
    pub who: LeaderSide,
    pub zone: CountZone,
    pub kind: Option<SearchKind>,
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
    pub cost: EvolutionCost,
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
    pub cannot_if: Option<PlayRestriction>,
    pub cost_delta: Option<Value>,
    pub cost_set: Option<Value>,
    pub optional_cost: Option<OptionalCost>,
    pub optional_cost_delta: Option<i64>,
    pub optional_cost_set: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PlayRestriction {
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

#[derive(Deserialize)]
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicQuery {
    pub racing: Option<bool>,
    #[serde(default)]
    pub other: bool,
    pub who: Side,
    pub zone: PublicZone,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub token: Option<bool>,
}

#[derive(Deserialize)]
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
    pub delta: Option<i64>,
    pub set: Option<i64>,
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
    EnteredFrom(EntryOriginCondition),
    EnteredBy(EntryCondition),
    PlayedFrom(PlayedFromCondition),
    Exists(PublicQuery),
    Awakened(bool),
    LeaderLife(PlayerThreshold),

    Crimson(bool),
    Combo(u64),
    Spellchain(u64),
    Necro(u64),
    Did(String),
    Paid(PaidReceipt),
    Cmp(BodyComparison),
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
    pub _recipient: This,
    pub kind: Option<AbilityDamage>,
    pub during: Option<DamageDuring>,
}
#[derive(Deserialize)]
pub(super) enum AbilityDamage {
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
    pub sel: HandQuery,
    pub n: u64,
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
    pub who: SelfPlayer,
    pub sel: PlayedSubject,
    pub during: During,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayedSubject {
    pub kind: PlayedSpell,
    pub class: String,
}
#[derive(Deserialize)]
pub(super) enum PlayedSpell {
    #[serde(rename = "spell")]
    Spell,
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
    #[serde(rename = "to")]
    pub _to: CemeteryDestination,
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
pub(super) enum CemeteryDestination {
    Cemetery,
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
    Grant(StaticGrant),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StaticGrant {
    pub on: Option<Objects>,
    pub keywords: Vec<String>,
}
