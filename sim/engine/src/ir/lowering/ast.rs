use alloc::collections::BTreeMap;
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
    #[serde(rename = "meta")]
    pub _meta: IgnoredAny,
    #[serde(rename = "note")]
    pub _note: Option<IgnoredAny>,
    #[serde(default)]
    pub abilities: Vec<Value>,
    #[serde(default, rename = "faces")]
    pub _faces: Empty,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ability {
    pub line: u32,
    pub section: Option<u64>,
    pub sources: [Value; 1],
    #[serde(rename = "ports")]
    pub _ports: Ports,
    pub imports: Option<IgnoredAny>,
    pub fanfare: Option<Body>,
    pub lastword: Option<Body>,
    pub attack_time: Option<Body>,
    pub auto: Option<Auto>,
    pub evolve_time: Option<Body>,
    pub evolve: Option<Evolution>,
    pub activate: Option<Activate>,
    pub spell: Option<Body>,
    pub play: Option<PlayCosts>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Activate {
    pub cost: Cost,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Cost {
    Short(CostOperation),
    Ordered { steps: Vec<CostOperation> },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
#[expect(
    variant_size_differences,
    reason = "Author cost operations have distinct scalar payloads."
)]
pub(super) enum CostOperation {
    Pp(u64),
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Body {
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
    If(Conditional),
    Choice(Choice),
    Draw(Draw),
    Damage(Damage),
    Destroy(Destroy),
    Buff(Modify),
    SetStat(Modify),
    Apply(Apply),
    Search(Search),
    Look(Look),
    Move(Move),
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
    pub r#ref: Observed,
    pub to: ObservedDestination,
    pub position: Option<Placement>,
    pub order: Option<Chooser>,
    pub result: Option<String>,
    #[serde(rename = "by")]
    pub _by: Option<SelfPlayer>,
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
pub(super) enum ObservedDestination {
    Deck,
    Ex,
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
pub(super) enum Destroy {
    Short(This),
    Long { r#ref: Objects },
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
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Objects {
    This(This),
    Zone(ObjectQuery),
    Follow { follow: String },
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObjectQuery {
    pub who: Side,
    pub zone: Zone,
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
    pub sel: Option<SearchQuery>,
    pub n: Option<u64>,
    pub upto: Option<u64>,
    pub groups: Option<Vec<SearchGroup>>,
    pub to: SearchDestination,
    pub result: Option<String>,
    pub distinct_by: Option<Names>,
}

#[derive(Deserialize)]
pub(super) enum Names {
    #[serde(rename = "name")]
    Name,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchGroup {
    pub sel: SearchQuery,
    pub n: Option<u64>,
    pub upto: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchQuery {
    pub who: SelfPlayer,
    pub zone: DeckZone,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SearchKind {
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
    pub to: Recipients,
    pub n: Number,
    pub distribute: Option<bool>,
    pub distribute_at: Option<AtPlay>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Recipients {
    Leaders { leaders: AllPlayers },
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
    pub n: Option<u64>,
    pub upto: Option<u64>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetQuery {
    pub who: Side,
    pub zone: Zone,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
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
#[serde(rename_all = "snake_case")]
pub(super) enum AllPlayers {
    All,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Number {
    Literal(i64),
    Alias(String),
    Minus { minus: Box<[Self; 2]> },
    Count { count: Deck },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Deck {
    pub who: SelfPlayer,
    pub zone: DeckZone,
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
    #[serde(flatten)]
    pub body: Body,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum PhaseSubscription {
    #[serde(rename = "phase")]
    Phase(PhaseSubscriptionPayload),
    #[serde(rename = "enter_field")]
    EnterField(EntrySubscription),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PhaseSubscriptionPayload {
    #[serde(rename = "which")]
    pub _which: EndPhase,
    pub whose: PhasePlayer,
}
#[derive(Deserialize)]
pub(super) enum EndPhase {
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
    pub sel: Option<EntrySubject>,
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
    #[serde(rename = "turn")]
    pub _turn: SelfPlayer,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EntrySubject {
    #[serde(rename = "kind")]
    pub _kind: Follower,
    #[serde(rename = "trait")]
    pub trait_id: Option<String>,
}
#[derive(Deserialize)]
pub(super) enum Follower {
    #[serde(rename = "follower")]
    Follower,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayCosts {
    pub cost_delta: Option<Value>,
    pub cost_set: Option<Value>,
    pub optional_cost: Option<OptionalCost>,
    pub optional_cost_delta: Option<i64>,
    pub optional_cost_set: Option<i64>,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicQuery {
    pub who: Side,
    pub zone: PublicZone,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PublicZone {
    Field,
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
    pub cond: PaidCondition,
    pub then: Vec<Step>,
    pub r#else: Option<Vec<Step>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PaidCondition {
    pub paid: PaidReceipt,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PaidReceipt {
    pub receipt: String,
}
