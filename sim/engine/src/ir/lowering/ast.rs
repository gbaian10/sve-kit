use crate::ir::{CardKind, Stat, ZoneKind};
use alloc::collections::{BTreeMap, BTreeSet};
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
    pub deck_rule: Option<DeckRule>,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeckRule {
    pub deck: DeckSection,
    pub same_name_max: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DeckSection {
    Main,
    Evolve,
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
    pub ride: Option<Ride>,
    pub drive_time: Option<Body>,
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
    pub advance: Option<True>,
    pub quick: Option<bool>,
    pub zone: Option<ActivationZones>,
    pub cost: Option<Cost>,
    pub fusion: Option<Fusion>,
    pub usable_if: Option<Condition>,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Fusion {
    pub sel: FusionQuery,
    pub n: Option<u64>,
    pub upto: Option<u64>,
    pub result: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FusionQuery {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
    pub zone: Vec<ActivationZone>,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    pub token: Option<bool>,
    pub not_token: Option<bool>,
    pub orig_cost_ge: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ActivationZones {
    One(ActivationZone),
    Many(Vec<ActivationZone>),
}

impl ActivationZones {
    pub(super) const fn single(&self) -> Option<ZoneKind> {
        match self {
            Self::One(zone) => Some(zone.kind()),
            Self::Many(zones) => match zones.as_slice() {
                [zone] => Some(zone.kind()),
                _ => None,
            },
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ActivationZone {
    Equipment,
    Hand,
    Cemetery,
    Field,
    Ex,
}

impl ActivationZone {
    pub(super) const fn kind(self) -> ZoneKind {
        match self {
            Self::Equipment => ZoneKind::Equipment,
            Self::Hand => ZoneKind::Hand,
            Self::Cemetery => ZoneKind::Cemetery,
            Self::Field => ZoneKind::Field,
            Self::Ex => ZoneKind::Ex,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayCard {
    pub r#ref: String,
    pub cost: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Cost {
    Short(Box<CostOperation>),
    Ordered { steps: Vec<CostOperation> },
}

impl Cost {
    pub(super) fn moves_self_to_ex(&self) -> bool {
        match self {
            Self::Short(operation) => matches!(**operation, CostOperation::SelfToEx(_)),
            Self::Ordered { steps } => steps
                .iter()
                .any(|operation| matches!(operation, CostOperation::SelfToEx(_))),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum PpAmount {
    Literal(u64),
    Variable(PpVariable),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PpVariable {
    X,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CostOperation {
    Pp(PpAmount),
    Lesson(u64),
    Counter(CounterCost),
    EarthRite(u64),
    LeaderLife(u64),
    ToCemetery(Material),
    ReturnToHand(Material),
    ToEx(ExMaterial),
    Banish(BanishMaterial),
    Act(Material),
    Discard(DiscardCost),
    Reveal(Box<HandMaterial>),
    ToDeckBottom(BottomMaterial),
    BanishSelf(bool),
    ActSelf(bool),
    SelfToCemetery(bool),
    /// 「手札のこれをEXエリアに置く」; only a hand activation pays it.
    SelfToEx(bool),
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum BanishMaterial {
    Public(Box<Material>),
    Hand(Box<HandMaterial>),
    Top(Box<TopBanishMaterial>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TopBanishMaterial {
    pub top: DeckTop,
    pub result: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum ExMaterial {
    Public(Box<Material>),
    Hand(Box<HandMaterial>),
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum BottomMaterial {
    Revealed(RevealedMaterial),
    Hand(Box<HandMaterial>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RevealedMaterial {
    pub sel: RevealedSelection,
    pub n: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RevealedSelection {
    pub r#in: String,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum DiscardCost {
    Material(Box<HandMaterial>),
    Groups(Box<HandMaterialGroups>),
    Reference { r#ref: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HandMaterialGroups {
    pub groups: Vec<HandMaterialGroup>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum HandMaterialGroup {
    Source {
        #[serde(rename = "sel")]
        _sel: HandSourceSelection,
        n: u64,
    },
    Material(Box<HandMaterial>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HandSourceSelection {
    #[serde(rename = "in")]
    pub _source: This,
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
    #[expect(
        clippy::zero_sized_map_values,
        reason = "The empty X declaration must be an object; any supplied key is rejected."
    )]
    pub x: Option<BTreeMap<String, IgnoredAny>>,
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
    #[serde(rename = "shuffle")]
    Shuffle(Shuffle),
    Delayed(Delayed),
    Simultaneous(Simultaneous),
    PlayCard(PlayCard),
    PlayAbility(PlayAbility),
    Pay(ModePayment),
    Counter(CounterChange),
    StackPlus(Number),
    Optional(Optional),
    Race(Race),
    GainDrive(EffectEvolution),
    Dice(Dice),
    Repeat(Repeat),
    Evolve(NamedEffectEvolution),
    If(Conditional),
    EachPlayer(EachPlayer),
    Choice(Choice),
    Either(Either),
    Choose(Choose),
    Draw(Draw),
    Win(Win),
    ExtraTurn(ExtraTurn),
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
    Steal(Steal),
    Give(Give),
    FlipDown(FlipDown),
    Buff(Modify),
    Grant(Grant),
    SetStat(Modify),
    Apply(Apply),
    PlayerEffect(PlayerEffect),
    Search(Search),
    Look(Look),
    Reveal(Reveal),
    RevealUntil(RevealUntil),
    Move(Move),
    Create(Create),
    Equip(Equip),
    Pilot(Pilot),
    #[serde(rename = "box")]
    Boxed(Boxed),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Simultaneous {
    #[serde(rename = "do")]
    pub steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum PlayAbility {
    Trigger {
        of: Objects,
        trigger: AbilityPlayTrigger,
        #[serde(default)]
        cost_policy: AbilityPlayCost,
    },
    Ability {
        of: Objects,
        ability: UbInvocation,
        #[serde(default)]
        cost_policy: AbilityPlayCost,
    },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AbilityPlayTrigger {
    EvolveTime,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UbInvocation {
    #[serde(rename = "ub")]
    pub _ub: True,
    pub n: u64,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AbilityPlayCost {
    #[default]
    Normal,
    WaiveOriginal,
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
    pub delta: Number,
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
    pub who: Option<Participant>,
    pub result: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Optional {
    #[serde(rename = "do")]
    pub steps: Vec<Step>,
    pub who: Option<Participant>,
    pub then: Option<Vec<Step>>,
}

/// Who answers or pays inside an effect; `master_of` is the chosen object's controller.
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum Participant {
    Side(LeaderSide),
    MasterOf {
        master_of: String,
    },
    /// The player of an enclosing `each_player`.
    Bound(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ride {
    pub cost: EvolutionCost,
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
pub(super) struct Boxed {
    #[serde(rename = "ref")]
    pub reference: String,
    pub until: Until,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pilot {
    #[serde(rename = "ref")]
    pub reference: This,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NamedEffectEvolution {
    #[serde(rename = "ref")]
    pub reference: Objects,
    pub into: Option<EvolutionDestination>,
    pub result: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Equip {
    pub on: This,
    pub token: String,
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
#[serde(untagged, deny_unknown_fields)]
pub(super) enum CreateToken {
    One(String),
    Many(Vec<String>),
    SameName {
        same_name_as: String,
        kind: Option<String>,
    },
}

impl CreateToken {
    pub(super) fn names(&self) -> Option<&[String]> {
        match self {
            Self::One(name) => Some(from_ref(name)),
            Self::Many(names) => Some(names),
            Self::SameName { .. } => None,
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
    Number(Box<Number>),
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
    pub who: LifeHolder,
    pub delta: Number,
}

/// Whose leader a life change reaches; `master_of` is the chosen object's controller.
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum LifeHolder {
    Controller(SelfPlayer),
    MasterOf { master_of: String },
    Bound(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceIncrease {
    pub who: SelfPlayer,
    pub delta: Number,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Discard {
    /// 「それを捨てる」: cards chosen earlier, discarded only while still in this player's hand.
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub n: Option<u64>,
    pub all: Option<bool>,
    pub who: Option<LeaderSide>,
    pub random: Option<bool>,
    pub any: Option<True>,
    pub result: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Win {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExtraTurn {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SetLife {
    pub who: SetLifeHolder,
    pub n: Number,
}

/// 「相手のリーダーすべての体力をNにする」 sets the opponent leader (BP02-075, BP10-075).
#[derive(Deserialize)]
pub(super) enum SetLifeHolder {
    #[serde(rename = "self")]
    Controller,
    #[serde(rename = "opponents")]
    Opponents,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Look {
    pub top: DeckTop,
    pub bind: String,
    /// Who looks; only the deck's own holder may look at it privately.
    pub by: Option<Participant>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeckTop {
    pub who: DeckHolder,
    pub n: u64,
}

/// Whose deck a fixed top slice reads; `owner_of` follows a chosen object's owner.
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum DeckHolder {
    Side(LeaderSide),
    Group(DeckGroup),
    OwnerOf { owner_of: String },
    Bound(String),
}

/// 「相手プレイヤーすべて」 keeps its printed set instead of collapsing to `opp`.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DeckGroup {
    Opponents,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Shuffle {
    pub who: DeckHolder,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WholeDeckReturn {
    pub who: DeckHolder,
    pub zone: ReturnZones,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ReturnZones {
    One(ReturnZone),
    Many(Vec<ReturnZone>),
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
pub(super) enum ReturnZone {
    #[serde(rename = "hand")]
    Hand,
    #[serde(rename = "cemetery")]
    Cemetery,
}

#[derive(Deserialize)]
pub(super) enum CemeteryZone {
    #[serde(rename = "cemetery")]
    Cemetery,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MoveOrder {
    Chooser,
    Shuffle,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Move {
    pub r#ref: Option<MoveObjects>,
    pub top: Option<DeckTop>,
    pub to: Destination,
    pub acted: Option<True>,
    pub position: Option<Placement>,
    pub order: Option<MoveOrder>,
    pub result: Option<String>,
    #[serde(rename = "by")]
    /// The performing player; a deck-edge placement's chooser is `position.by`.
    pub by: Option<SelfPlayer>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum MoveObjects {
    Whole(WholeDeckReturn),
    Observed(Observed),
    Field(Objects),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Observed {
    pub r#in: Observation,
    pub except: Option<ObservedExcept>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum ObservedExcept {
    Atom(Box<Observed>),
    Union { any_of: Vec<Observed> },
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
pub(super) enum Placement {
    Chosen {
        chosen: String,
    },
    Edge(DeckEdge),
    Nth {
        nth: u64,
    },
    /// A player assigns each moved object one of these placements.
    OneOf {
        one_of: Vec<Self>,
        by: Participant,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DeckEdge {
    Top,
    Bottom,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum BanishAction {
    Top { top: DeckTop },
    Union { r#ref: PublicBanishUnion },
    Objects(Box<ObjectAction>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicBanishUnion {
    pub any_of: Vec<PublicBanishZone>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicBanishZone {
    pub who: Side,
    pub zone: Zone,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum FlipDown {
    Public {
        r#ref: CountQuery,
        result: Option<String>,
    },
    Selected(ObjectAction),
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
    pub instead: Option<Vec<PowerInstead>>,
    pub on: NumericSubjects,
    pub power: Option<Number>,
    pub hp: Option<Number>,
    pub until: Option<Until>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum NumericSubjects {
    Single(Objects),
    Union { any_of: Box<[ObjectQuery; 2]> },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PowerInstead {
    pub r#if: Condition,
    pub power: Number,
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
    Zone(Box<ObjectQuery>),
    Follow { follow: String },
    EventHolder { event: HolderEvent },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HolderEvent {
    Holder,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObjectQuery {
    #[serde(rename = "as")]
    pub alias: Option<String>,
    pub r#where: Option<ObjectQueryWhere>,
    pub who: Side,
    pub zone: Zone,
    pub kind: Option<FieldKind>,
    pub r#trait: Option<String>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub token: Option<bool>,
    pub not_token: Option<bool>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_le: Option<u64>,
    pub orig_cost_ge: Option<u64>,
    pub hp_le: Option<u64>,
    pub not_trait: Option<String>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub acted: Option<bool>,
    pub except: Option<Box<BoundExcept>>,
    #[serde(default)]
    pub other: bool,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum ObjectQueryWhere {
    Extremum(Box<QueryExtremumWhere>),
    Damaged(DamagedThisTurnWhere),
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamagedThisTurnWhere {
    pub history: DamagedThisTurnHistory,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamagedThisTurnHistory {
    pub event: DamagedHistoryEvent,
    pub window: ThisTurn,
    pub r#where: DamagedThisTurnRecipient,
}
#[derive(Clone, Deserialize)]
pub(super) enum DamagedHistoryEvent {
    #[serde(rename = "damaged")]
    Damaged,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamagedThisTurnRecipient {
    pub recipient: Item,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum FieldKind {
    Follower,
    Amulet,
}

impl FieldKind {
    pub(super) const fn search(self) -> SearchKind {
        match self {
            Self::Follower => SearchKind::Follower,
            Self::Amulet => SearchKind::Amulet,
        }
    }
}

/// `except: {in: binding}` removes an already chosen target from a zone source.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BoundExcept {
    #[serde(rename = "in")]
    pub reference: String,
}

/// Whose objects a query reads; a binding name is the player of an enclosing `each_player`.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Side {
    Controller,
    Opponent,
    All,
    Bound(String),
}

impl Side {
    fn named(name: &str) -> Self {
        match name {
            "self" => Self::Controller,
            "opp" => Self::Opponent,
            "all" => Self::All,
            name => Self::Bound(name.to_owned()),
        }
    }

    /// The relation to the effect controller; a procedure's player has none.
    pub(super) const fn relation(&self) -> Option<super::super::PlayerRelation> {
        match self {
            Self::Controller => Some(super::super::PlayerRelation::SelfPlayer),
            Self::Opponent => Some(super::super::PlayerRelation::Opponents),
            Self::All => Some(super::super::PlayerRelation::All),
            Self::Bound(_) => None,
        }
    }
}

impl<'de> Deserialize<'de> for Side {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::named(&String::deserialize(deserializer)?))
    }
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
    PlayPermission(PlayPermission),
    LoseAbilities(Empty),
    DamageMod(DamageModifier),
    AddTrait(TraitEffect),
    Stat(ConstantStats),
    CostMod(CostModifier),
    EvolveCostMod(EvolutionCostModifier),
    Cannot(Cannot),
    Prevent(Prevent),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TraitEffect {
    pub r#trait: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CannotStand {
    pub what: StandAction,
    pub on: StandSubject,
    pub during: StandDuring,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StandSubject {
    This,
    Affected,
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
    pub on: EvolutionCostSubject,
    pub delta: Option<Value>,
    pub set: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CostModifier {
    pub when_playing: Playing,
    pub delta: Option<Value>,
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
    pub acted: Option<True>,
    pub result: Option<String>,
    pub distinct_by: Option<Names>,
    pub cost_sum_le: Option<u64>,
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
    pub cost_le: Option<NumericArgument>,
    pub cost_ge: Option<u64>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_le: Option<NumericArgument>,
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
    /// `master_of` is 「それのプレイヤー」, the chosen object's controller (BP06-112).
    pub who: Option<LifeHolder>,
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
    Role { role: RoleReference },
    Union { any_of: Vec<Self> },
    Field(Box<ObjectQuery>),
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
#[serde(try_from = "RawTarget")]
pub(super) struct Target {
    pub any: bool,
    pub option: Option<u64>,
    pub bind: String,
    /// For a union this keeps only what every branch shares, so per-binding bookkeeping
    /// never assumes a filter that only one branch has.
    pub from: TargetQuery,
    /// 「AかB」 branches of one candidate set; a card matching several is still one choice.
    pub any_of: Option<Vec<TargetQuery>>,
    pub groups: Option<Vec<FormalTargetGroup>>,
    pub distinct_by: Option<Names>,
    pub cost_sum_le: Option<u64>,
    pub roles: Option<BTreeMap<String, TargetRole>>,
    pub n: Option<NumericArgument>,
    pub upto: Option<NumericArgument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawTarget {
    #[serde(default)]
    any: bool,
    option: Option<u64>,
    bind: String,
    from: TargetFrom,
    groups: Option<Vec<FormalTargetGroup>>,
    distinct_by: Option<Names>,
    cost_sum_le: Option<u64>,
    roles: Option<BTreeMap<String, TargetRole>>,
    n: Option<NumericArgument>,
    upto: Option<NumericArgument>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum TargetFrom {
    Union { any_of: Vec<TargetQuery> },
    Query(Box<TargetQuery>),
}

fn shared<T>(branches: &[TargetQuery], field: impl Fn(&TargetQuery) -> &Option<T>) -> Option<T>
where
    T: Clone + PartialEq,
{
    let (first, rest) = branches.split_first()?;
    let value = field(first).as_ref()?;
    rest.iter()
        .all(|branch| field(branch).as_ref() == Some(value))
        .then(|| value.clone())
}

impl TryFrom<RawTarget> for Target {
    type Error = &'static str;

    fn try_from(raw: RawTarget) -> Result<Self, Self::Error> {
        let (from, any_of) = match raw.from {
            TargetFrom::Query(query) => (*query, None),
            TargetFrom::Union { any_of } => {
                if any_of.len() < 2 {
                    return Err("target any_of needs at least two branches");
                }
                let common = TargetQuery {
                    who: shared(&any_of, |branch| &branch.who),
                    zone: shared(&any_of, |branch| &branch.zone),
                    kind: shared(&any_of, |branch| &branch.kind),
                    ..TargetQuery::default()
                };
                (common, Some(any_of))
            }
        };
        Ok(Self {
            any: raw.any,
            option: raw.option,
            bind: raw.bind,
            from,
            any_of,
            groups: raw.groups,
            distinct_by: raw.distinct_by,
            cost_sum_le: raw.cost_sum_le,
            roles: raw.roles,
            n: raw.n,
            upto: raw.upto,
        })
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalTargetGroup {
    pub r#trait: Option<String>,
    pub not_class: Option<String>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_le: Option<u64>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum TargetRole {
    IfNonempty { if_nonempty: u64 },
    Remaining(RemainingRole),
}

#[derive(Clone, Deserialize)]
pub(super) enum RemainingRole {
    #[serde(rename = "remaining")]
    Remaining,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RoleReference {
    pub of: String,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Choose {
    pub position: Option<HandPosition>,
    #[serde(default)]
    pub any: bool,
    pub bind: String,
    pub from: ChooseFrom,
    pub groups: Option<Vec<ChooseGroup>>,
    pub distinct_by: Option<Names>,
    pub cost_sum_le: Option<u64>,
    pub n: Option<u64>,
    pub upto: Option<u64>,
    pub may: Option<bool>,
    pub reveal: Option<bool>,
    pub order: Option<bool>,
    pub by: Option<Participant>,
}

#[derive(Deserialize)]
pub(super) enum HandPosition {
    #[serde(rename = "top_or_bottom")]
    TopOrBottom,
}

/// A union keeps one candidate set: a card matching several branches is still one choice.
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum ChooseFrom {
    Union { any_of: Vec<TargetQuery> },
    Advance(Box<AdvanceEntryQuery>),
    Query(Box<TargetQuery>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdvanceEntryQuery {
    pub who: SelfPlayer,
    pub zone: EvolveDeckZone,
    pub kind: SearchKind,
    pub advance_follower: Option<True>,
    pub advance_spell: Option<True>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EvolveDeckZone {
    EvolveDeck,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChooseGroup {
    pub kind: Option<SearchKind>,
    pub name: Option<String>,
    pub r#trait: Option<String>,
    pub class: Option<String>,
    pub orig_cost_le: Option<u64>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetQuery {
    pub except: Option<ObservedNameExcept>,
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
    pub traits_any: Option<TraitUnion>,
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
    pub cost_le: Option<NumericArgument>,
    pub cost_ge: Option<u64>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_le: Option<NumericArgument>,
    pub orig_cost_ge: Option<u64>,
    pub hp_le: Option<u64>,
    pub power_le: Option<u64>,
    pub power_ge: Option<u64>,
    pub has_evolve: Option<True>,
    pub boxed: Option<bool>,
    pub acted: Option<bool>,
    pub evolved: Option<bool>,
    pub entered_this_turn: Option<True>,
    pub face_up: Option<bool>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String")]
pub(super) struct FilterKeyword(pub super::super::KeywordId);

impl TryFrom<String> for FilterKeyword {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match super::keywords::ability_filter_id(&value)
            .or_else(|| super::keywords::keyword_id(&value))
        {
            Some(
                keyword @ (super::super::KeywordId::EarthRite
                | super::super::KeywordId::Lastword
                | super::super::KeywordId::Ward
                | super::super::KeywordId::Aura
                | super::super::KeywordId::Storm
                | super::super::KeywordId::Rush
                | super::super::KeywordId::Bane
                | super::super::KeywordId::Intimidate
                | super::super::KeywordId::Assail
                | super::super::KeywordId::Stack),
            ) => Ok(Self(keyword)),
            _ => Err("unsupported filter keyword"),
        }
    }
}

/// 「A・カードかB・カード」: at least two distinct, non-empty trait names, any of which matches.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(try_from = "Vec<String>")]
pub(super) struct TraitUnion(pub Vec<String>);

impl TryFrom<Vec<String>> for TraitUnion {
    type Error = &'static str;

    fn try_from(value: Vec<String>) -> Result<Self, Self::Error> {
        let distinct: BTreeSet<_> = value.iter().collect();
        if value.len() < 2 || distinct.len() != value.len() || value.iter().any(String::is_empty) {
            return Err("traits_any needs at least two distinct trait names");
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObservedNameExcept {
    #[serde(rename = "in")]
    pub observation: String,
    pub name: String,
}

impl TargetQuery {
    /// Field-state filters exist only on current field objects, not leaders or looked cards.
    pub(super) const fn has_state_filter(&self) -> bool {
        self.acted.is_some()
            || self.evolved.is_some()
            || self.entered_this_turn.is_some()
            || self.boxed.is_some()
    }

    pub(super) fn has_numeric_filter(&self) -> bool {
        self.cost_le.is_some()
            || self.orig_cost_le.is_some()
            || self.alias.is_some()
            || self.r#where.is_some()
            || [
                self.cost_eq,
                self.cost_ge,
                self.orig_cost_eq,
                self.orig_cost_ge,
                self.hp_le,
                self.power_le,
                self.power_ge,
            ]
            .into_iter()
            .any(|limit| limit.is_some())
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub(super) enum TargetZone {
    Single(TargetSingleZone),
    PublicPair(Vec<TargetSingleZone>),
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TargetSingleZone {
    EvolveDeck,
    Hand,
    Field,
    Ex,
    Cemetery,
    /// 「自分の消滅領域の…を選ぶ」; the banish zone is public.
    Banish,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Choice {
    pub n: Option<u64>,
    pub upto: Option<ModeCap>,
    pub instead: Option<Vec<ChoiceInstead>>,
    pub exclude_chosen: Option<ThisTurn>,
    pub options: Vec<Vec<Step>>,
    #[serde(rename = "who")]
    pub _who: Option<SelfPlayer>,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum ModeCap {
    Fixed(u64),
    Count { count: CountQuery },
    // Structured counts retain their narrower admission before the numeric fallback.
    Number(Box<Value>),
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
    Dice {
        dice: String,
    },
    Literal(i64),
    PaidAmount {
        x_paid: String,
    },
    Counter {
        counter: CounterRead,
    },
    Alias(String),
    Power {
        power_of: Observation,
    },
    Life {
        hp_of: BodyStatReference,
    },
    Cost {
        cost_of: Observation,
    },
    OriginalCost {
        orig_cost_of: This,
    },
    HistoryCount {
        history_count: HistoryQuery,
    },
    HistorySum {
        history_sum: LifeHistorySum,
    },
    LeaderLife {
        leader_life: LeaderSide,
    },
    Plus {
        plus: Box<[Self; 2]>,
    },
    Minus {
        minus: Box<[Self; 2]>,
    },
    Times {
        times: Box<[Self; 2]>,
    },
    HalfUp {
        half_up: Box<Self>,
    },
    Per {
        per: Box<Per>,
    },
    Min {
        min: Vec<Self>,
    },
    Count {
        count: CountQuery,
    },
    ReceiptCount {
        count: ReceiptCountQuery,
    },
    CountOf {
        count_of: CountOf,
    },
    Aggregate {
        agg: Aggregate,
    },
    DistinctNames {
        distinct_names: DistinctQuery,
    },
    DistinctOriginalCosts {
        distinct_orig_costs: DistinctQuery,
    },
    Resource {
        resource: ResourceQuery,
    },
    TurnOrdinal {
        turn_number: TurnNumberQuery,
    },
    Resolution {
        at: NumericCaptureTime,
        value: Box<Self>,
    },
    Selection {
        at: SelectionTime,
        selection: String,
        value: SelectedStat,
    },
    Event {
        event: EventNumber,
    },
}

/// A numeric field of the typed triggering event.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EventNumber {
    Amount,
    Value,
}

/// 「N枚につき」: how many whole `unit`s fit in `of`, rounded down.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Per {
    pub of: Number,
    pub unit: u64,
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
    pub sel: Option<HistorySubject>,
    pub from: Option<HistoryOrigin>,
    pub to: Option<DepartureDestination>,
    pub at_least: Option<u64>,
    pub r#where: Option<LifeHistoryFilter>,
    pub exclude: Option<HistoryExclusion>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum HistorySubject {
    // The shared single-subject AST must not consume the closed history union.
    Union(HistoryTraitUnion),
    Single(MovementSubject),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryTraitUnion {
    pub traits_any: [String; 2],
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HistoryExclusion {
    ThisExecution,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LifeHistoryFilter {
    pub direction: LifeDirection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LifeHistorySum {
    pub event: LifeHistoryEvent,
    pub window: ThisTurn,
    pub who: LeaderSide,
    pub r#where: LifeHistoryFilter,
    pub field: LifeHistoryField,
}
#[derive(Deserialize)]
pub(super) enum LifeHistoryEvent {
    #[serde(rename = "life_changed")]
    LifeChanged,
}
#[derive(Deserialize)]
pub(super) enum LifeHistoryField {
    #[serde(rename = "delta")]
    Delta,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HistoryEvent {
    Banished,
    LifeChanged,
    Played,
    Discarded,
    UbActivated,
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
    Hand(HandZone),
    Ex(HistoryExZone),
}
#[derive(Deserialize)]
pub(super) enum HistoryExZone {
    #[serde(rename = "ex")]
    Ex,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HistoryDistinct {
    Facts,
    Subjects,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NumericCaptureTime {
    Resolution,
    Event,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SelectionTime {
    Selection,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SelectedStat {
    #[serde(rename = "distinct_orig_costs")]
    DistinctOriginalCosts(SelectedCollection),
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
pub(super) struct SelectedCollection {
    pub r#in: String,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CountKind {
    EvolveFollower,
    Follower,
    Spell,
    Amulet,
    Crest,
}
impl CountKind {
    pub(super) const fn kind(self) -> CardKind {
        match self {
            Self::EvolveFollower => CardKind::EvolveFollower,
            Self::Follower => CardKind::Follower,
            Self::Spell => CardKind::Spell,
            Self::Amulet => CardKind::Amulet,
            Self::Crest => CardKind::Crest,
        }
    }
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
    pub kind: Option<CountKind>,
    pub keyword: Option<FilterKeyword>,
    pub r#trait: Option<String>,
    /// 「AやBの…」: a public count of cards having either trait (BP09-117).
    pub traits_any: Option<TraitUnion>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_ge: Option<u64>,
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
    pub aggregation: Option<AutoAggregation>,
    pub zone: Option<ActivationZones>,
    #[serde(flatten)]
    pub body: Body,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AutoAggregation {
    Batch,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum PhaseSubscription {
    #[serde(rename = "fused")]
    Fused(FusedSubscription),
    #[serde(rename = "fused_as_material")]
    FusedAsMaterial(FusedAsMaterialSubscription),
    #[serde(rename = "any_of")]
    AnyOf(Vec<Self>),
    #[serde(rename = "equipped")]
    Equipped(EquippedSubscription),
    #[serde(rename = "drew")]
    Drew(DrawSubscription),
    #[serde(rename = "drive_checked")]
    DriveChecked(DriveCheckedSubscription),
    #[serde(rename = "drive_trigger")]
    DriveTrigger(DriveTriggerSubscription),
    #[serde(rename = "dice_rolled")]
    DiceRolled(DiceSubscription),
    #[serde(rename = "discarded")]
    Discarded(DiscardSubscription),
    #[serde(rename = "banished")]
    Banished(HandBanishSubscription),
    #[serde(rename = "evolved")]
    Evolved(EvolvedSubscription),
    #[serde(rename = "played")]
    Played(PlayedSubscription),
    #[serde(rename = "ub_activated")]
    UbActivated(UbSubscription),
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
pub(super) struct FusedSubscription {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
    pub material: FusedMaterial,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FusedAsMaterialSubscription {
    #[serde(rename = "ref")]
    pub reference: This,
    pub by: FusedAsMaterialBy,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FusedAsMaterialBy {
    pub ability_of: FusedAsMaterialSource,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FusedAsMaterialSource {
    pub who: SelfPlayer,
    pub kind: SearchKind,
    pub r#trait: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FusedMaterial {
    pub r#trait: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EquippedSubscription {
    #[serde(rename = "equipment")]
    pub _equipment: This,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DriveCheckedSubscription {
    #[serde(rename = "holder")]
    pub _holder: DriveHolder,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DriveHolder {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
    #[serde(rename = "zone")]
    pub _zone: FieldHolderZone,
    #[serde(rename = "kind")]
    pub _kind: FollowerKind,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DriveTriggerSubscription {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
    pub during: Option<During>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DrawSubscription {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
    pub not_phase: StartPhase,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UbSubscription {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
    #[serde(rename = "holder")]
    pub _holder: UbHolder,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UbHolder {
    #[serde(rename = "who")]
    pub _who: SelfPlayer,
    #[serde(rename = "zone")]
    pub _zone: FieldHolderZone,
    #[serde(rename = "kind")]
    pub _kind: FollowerKind,
    #[serde(rename = "other")]
    pub _other: True,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FieldHolderZone {
    Field,
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
    /// A phase always belongs to the turn player, so this is every player's own phase.
    #[serde(rename = "turn")]
    Turn,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evolution {
    pub cost: Cost,
    pub into: Option<EvolutionDestination>,
    pub usable_if: Option<Condition>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum EvolutionDestination {
    Name(String),
    Names(Vec<String>),
    Contains(EvolutionNameSelector),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvolutionNameSelector {
    pub who: SelfPlayer,
    pub zone: EvolutionZone,
    pub kind: SearchKind,
    pub name_contains: String,
}

#[derive(Deserialize)]
pub(super) enum EvolutionZone {
    #[serde(rename = "evolve_deck")]
    Evolve,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvolutionCost {
    pub pp: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiscardSubscription {
    #[serde(rename = "ref")]
    pub reference: Option<This>,
    #[serde(rename = "from")]
    pub from: Option<HandZone>,
    pub who: Option<EntryPlayer>,
    pub during: Option<During>,
    pub by: Option<DiscardBy>,
    pub sel: Option<MovementSubject>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HandBanishSubscription {
    #[serde(rename = "ref")]
    pub reference: This,
    #[serde(rename = "from")]
    pub _from: HandZone,
    #[serde(rename = "who")]
    pub _who: Option<SelfPlayer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiscardBy {
    pub ability_of: DiscardAbility,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiscardAbility {
    pub who: SelfPlayer,
    pub r#trait: String,
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
#[derive(Clone, Deserialize)]
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayCosts {
    #[serde(default)]
    pub define: BTreeMap<String, Number>,
    pub cannot_if: Option<PlayRestriction>,
    pub playable_from: Option<Box<ConditionalCemetery>>,
    pub cost_delta: Option<Value>,
    pub cost_set: Option<Value>,
    pub additional_cost: Option<AdditionalCost>,
    pub optional_cost: Option<OptionalCost>,
    pub optional_cost_delta: Option<i64>,
    pub optional_cost_set: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConditionalCemetery {
    #[serde(rename = "zone")]
    pub _zone: CemeteryOnly,
    #[serde(rename = "if")]
    pub condition: Condition,
}

#[derive(Deserialize)]
pub(super) enum CemeteryOnly {
    #[serde(rename = "cemetery")]
    Cemetery,
}

#[derive(Deserialize)]
pub(super) struct AdditionalCost {
    #[serde(flatten)]
    pub cost: Cost,
    pub result: Option<String>,
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
    History(Box<HistoryQuery>),
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
    pub acted: Option<bool>,
    pub evolved: Option<bool>,
    pub racing: Option<bool>,
    #[serde(default)]
    pub other: bool,
    #[serde(default)]
    pub not_same_name_as_this: bool,
    pub who: Side,
    pub zone: PublicZone,
    pub kind: Option<SearchKind>,
    pub keyword: Option<FilterKeyword>,
    pub r#trait: Option<String>,
    pub traits_any: Option<TraitUnion>,
    #[serde(default)]
    pub traits_all: Vec<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub names: Vec<String>,
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
    pub r#where: Option<PublicPowerWhere>,
}

/// `where: {cmp: {left: {power_of: item}, op: ge|le, right: N}}` on each public candidate.
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicPowerWhere {
    pub cmp: PublicPowerComparison,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicPowerComparison {
    pub left: ItemPower,
    pub op: PowerBound,
    pub right: u64,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ItemPower {
    #[serde(rename = "power_of")]
    pub _power_of: Item,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Item {
    Item,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum PowerBound {
    Ge,
    Le,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ExistsQuery {
    Public(PublicQuery),
    Selected(SelectedQuery),
    FollowedKeyword(FollowedKeywordQuery),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FollowedKeywordQuery {
    pub r#in: FollowName,
    pub keyword: FilterKeyword,
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
#[serde(try_from = "PublicZones")]
pub(super) enum PublicZone {
    Field,
    Ex,
    Cemetery,
    FieldEx,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PublicZones {
    One(PublicZoneName),
    Many(Vec<PublicZoneName>),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PublicZoneName {
    Field,
    Ex,
    Cemetery,
}

impl TryFrom<PublicZones> for PublicZone {
    type Error = &'static str;

    fn try_from(zones: PublicZones) -> Result<Self, Self::Error> {
        Ok(match zones {
            PublicZones::One(PublicZoneName::Field) => Self::Field,
            PublicZones::One(PublicZoneName::Ex) => Self::Ex,
            PublicZones::One(PublicZoneName::Cemetery) => Self::Cemetery,
            PublicZones::Many(zones) => match zones.as_slice() {
                [PublicZoneName::Field, PublicZoneName::Ex]
                | [PublicZoneName::Ex, PublicZoneName::Field] => Self::FieldEx,
                _ => return Err("unsupported public zone union"),
            },
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConditionalChange {
    pub r#if: CostCondition,
    pub delta: Option<Number>,
    pub set: Option<Number>,
}

#[derive(Clone, Deserialize)]
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
    Cmp(Box<BodyComparison>),
    History(HistoryQuery),
    Spellchain(u64),
    Necro(u64),
    EnteredFrom(EntryOriginCondition),
    LeaderLife(PlayerThreshold),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OptionalCost {
    pub pp: Option<u64>,
    pub one_of: Option<Vec<Self>>,
    pub discard: Option<HandMaterial>,
    pub to_cemetery: Option<Material>,
    pub banish: Option<Material>,
    pub act: Option<Material>,
    pub result: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Material {
    pub sel: PublicQuery,
    pub n: Option<u64>,
    #[serde(default)]
    pub any: bool,
    pub result: Option<String>,
    pub distinct_by: Option<MaterialDistinct>,
    /// 元のコストの合計がN以上になるように: a floor on the chosen materials' printed costs.
    pub cost_sum_ge: Option<u64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MaterialDistinct {
    OrigCost,
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
    MyTurn(bool),
    InZone(PlayRestrictionZone),
    Is(AttackTargetCondition),
    History(HistoryCondition),
    EnteredFrom(EntryOriginCondition),
    EnteredBy(EntryCondition),
    PlayedFrom(PlayedFromCondition),
    Exists(Box<ExistsQuery>),
    Standing(This),
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
    pub reference: EntryOriginReference,
    #[serde(rename = "from")]
    pub zone: EntryOriginZone,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EntryOriginReference {
    This,
    It,
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
    pub r#ref: Option<RevealRef>,
    pub top: Option<DeckTop>,
    pub bind: String,
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
    #[serde(default)]
    pub traits_all: Vec<String>,
    pub traits_any: Option<TraitUnion>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub keyword: Option<String>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_ge: Option<u64>,
    pub orig_cost_le: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Grant {
    pub on: Objects,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub abilities: Vec<Value>,
    pub until: Option<Until>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayedSubscription {
    pub who: Option<EntryPlayer>,
    #[serde(default)]
    pub sel: Option<PlayedSubject>,
    #[serde(default)]
    pub during: Option<During>,
}
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum PlayedSubject {
    Single(MovementSubject),
    Union { any_of: Box<[MovementSubject; 2]> },
}
impl Auto {
    /// 「自分の場に…が出たとき」 or a Play observed by a card in its owner's cemetery.
    pub(super) const fn cemetery_observer(&self) -> bool {
        matches!(
            self.on,
            PhaseSubscription::Played(_) | PhaseSubscription::EnterField(_)
        ) && match &self.zone {
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
    Zone(MoveZone),
    Owned { who: EntryPlayer, zone: MoveZone },
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum MoveZone {
    Field,
    /// Only a public deck-to-cemetery move; the deck owner must be named.
    Deck,
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
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MovementSubject {
    pub r#where: Option<Box<SourceLinkCondition>>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_le: Option<u64>,
    pub orig_cost_ge: Option<u64>,
    pub traits_any: Option<Vec<String>>,
    pub keyword: Option<FilterKeyword>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub kind: Option<SearchKind>,
    #[serde(rename = "trait")]
    pub trait_id: Option<String>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub token: Option<bool>,
    pub not_token: Option<bool>,
    pub other: Option<bool>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceLinkCondition {
    pub history: SourceLinkHistory,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceLinkHistory {
    pub event: DealtDamageHistory,
    pub window: ThisTurn,
    pub who: Option<LeaderSide>,
    pub sel: Option<SourceLinkSubject>,
    pub r#where: SourceLinkWhere,
}
#[derive(Clone, Deserialize)]
pub(super) enum DealtDamageHistory {
    #[serde(rename = "dealt_damage")]
    DealtDamage,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceLinkSubject {
    pub r#trait: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceLinkWhere {
    pub recipient: Item,
    #[serde(rename = "ref")]
    pub reference: Option<This>,
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
    BanishInstead(BanishReplacement),
    UnlimitedEvolve(StaticPlayerRule),
    ChoiceAny(StaticPlayerRule),
    Reroll(Reroll),
    AlsoNamed(AlsoNamed),
    If(Box<StaticConditional>),
    EvolveCostMod(EvolutionCostModifier),
    CostMod(CostModifier),
    Grant(StaticGrant),
    Cannot(Cannot),
    Prevent(Prevent),
    DamageMod(DamageModifier),
    DamageCap(DamageCap),
    NoTrigger(NoTrigger),
    IgnoreWard(IgnoreWard),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BanishReplacement {
    pub on: Option<Objects>,
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StaticPlayerRule {
    pub who: SelfPlayer,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IgnoreWard {
    pub on: Objects,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StaticConditional {
    pub cond: Condition,
    pub then: [StaticConditionalEffect; 1],
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StaticConditionalEffect {
    Grant(StaticGrant),
    Prevent(Prevent),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AlsoNamed {
    pub on: Option<Objects>,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StaticGrant {
    pub on: Option<Objects>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub abilities: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Cannot {
    Player(CannotPlayer),
    PlayAbility(CannotPlayAbility),
    Removal(CannotRemoval),
    Stand(CannotStand),
    DealDamage(CannotDealDamage),
    Attack(Box<CannotAttack>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CannotPlayer {
    pub what: ResultAction,
    pub who: EntryPlayer,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ResultAction {
    Win,
    Lose,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CannotPlayAbility {
    #[serde(rename = "what")]
    pub _what: PlayAbilityAction,
    pub ability: ActivatedPredicate,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PlayAbilityAction {
    PlayAbility,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActivatedPredicate {
    #[serde(rename = "kind")]
    pub _kind: ActivatedKind,
    pub except: Option<Vec<EvolutionException>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ActivatedKind {
    Activated,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EvolutionException {
    Evolution,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CannotRemoval {
    pub what: RemovalAction,
    pub on: Option<Objects>,
    pub by: Option<RemovalBy>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RemovalAction {
    Destroy,
    Banish,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RemovalBy {
    Ability,
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
#[serde(untagged)]
pub(super) enum PreventionSource {
    Objects(Box<Objects>),
    Controller(Box<PreventionController>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreventionController {
    pub who: LeaderSide,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prevent {
    pub on: Option<Recipients>,
    pub kind: PreventionKind,
    pub source: Option<PreventionSource>,
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
    pub of: AggregateOf,
    pub attr: AggregateAttribute,
    #[serde(rename = "fn")]
    pub function: AggregateFunction,
}
#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum AggregateOf {
    Public(Box<PublicQuery>),
    Followed(FollowedIn),
    Observed(ObservedAggregate),
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObservedAggregate {
    pub r#in: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FollowedIn {
    pub r#in: FollowName,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FollowName {
    pub follow: String,
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
#[serde(untagged, deny_unknown_fields)]
pub(super) enum DistinctQuery {
    Receipt { r#in: BeforeObjects },
    Public(Box<PublicQuery>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReceiptCountQuery {
    pub r#in: BeforeObjects,
    pub kind: Option<SearchKind>,
    pub r#trait: Option<String>,
    pub class: Option<String>,
    pub not_class: Option<String>,
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub orig_cost_eq: Option<u64>,
    pub orig_cost_ge: Option<u64>,
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
    pub source: Option<PreventionSource>,
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
    pub zone: Option<Zone>,
    pub sel: Option<MovementSubject>,
    pub recipient: Option<DealtRecipientSubscription>,
    pub kind: Option<AbilityDamage>,
    pub during: Option<DamageDuring>,
}
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum DealtRecipientSubscription {
    Basic(DamageRecipientSubscription),
    Followers {
        who: EntryPlayer,
        zone: Zone,
        kind: FollowerKind,
    },
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
    #[serde(default)]
    pub crosses_zero: bool,
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

impl StatSubscription {
    pub(super) fn properties(&self) -> Vec<Stat> {
        let stats = match &self.stat {
            None => return vec![Stat::Attack, Stat::Life],
            Some(ChangeStats::One(stat)) => from_ref(stat),
            Some(ChangeStats::Many(stats)) => stats,
        };
        stats
            .iter()
            .map(|stat| match stat {
                ChangeStat::Power => Stat::Attack,
                ChangeStat::Hp => Stat::Life,
            })
            .collect()
    }
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

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum NumericArgument {
    Literal(u64),
    Expression(Box<Value>),
}
impl NumericArgument {
    pub(super) const fn literal(&self) -> Option<u64> {
        match self {
            Self::Literal(value) => Some(*value),
            Self::Expression(_) => None,
        }
    }
    pub(super) const fn is_dynamic(&self) -> bool {
        matches!(self, Self::Expression(_))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EvolutionCostSubject {
    This,
    Affected,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum CountOf {
    Named(String),
    Filtered(FilteredCountOf),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FilteredCountOf {
    pub receipt: String,
    #[serde(rename = "where")]
    pub filter: ReceiptTraitFilter,
    #[serde(rename = "view")]
    pub _view: Before,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReceiptTraitFilter {
    pub r#trait: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoTrigger {
    pub who: Option<EntryPlayer>,
    pub on: Option<This>,
    pub kinds: Vec<NoTriggerKind>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NoTriggerKind {
    Fanfare,
    EvolveTime,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RevealUntil {
    pub sel: RevealUntilSelector,
    pub n: u64,
    pub bind: String,
    pub matched: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RevealUntilSelector {
    pub who: SelfPlayer,
    pub zone: DeckZone,
    pub kind: RevealFollower,
    pub orig_cost_le: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RevealFollower {
    Follower,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Dice {
    pub bind: String,
    pub who: Option<SelfPlayer>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Repeat {
    #[serde(rename = "n")]
    pub times: u64,
    #[serde(rename = "do")]
    pub steps: Vec<Step>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiceSubscription {
    pub who: SelfPlayer,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reroll {
    pub who: SelfPlayer,
    pub times: u64,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum HistoryCondition {
    Dice(DiceHistory),
    StatChanged(StatGainHistory),
    Other(HistoryQuery),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StatGainHistory {
    #[serde(rename = "event")]
    pub _event: StatGainEvent,
    #[serde(rename = "window")]
    pub _window: ThisTurn,
    pub r#where: StatSubscription,
    pub at_least: Option<u64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StatGainEvent {
    StatChanged,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiceHistory {
    pub event: DiceHistoryEvent,
    pub who: SelfPlayer,
    pub window: ThisTurn,
    pub r#where: Option<DiceHistoryWhere>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DiceHistoryEvent {
    DiceRolled,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiceHistoryWhere {
    pub value: DiceThreshold,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DiceThreshold {
    Eq(u64),
    Le(u64),
    Ge(u64),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Delayed {
    pub on: DelayedOn,
    #[serde(rename = "do")]
    pub steps: Vec<Step>,
    pub until: Until,
    #[serde(default)]
    pub next_only: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DelayedOn {
    pub moved: DelayedMove,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DelayedMove {
    pub r#ref: String,
    pub from: FieldZone,
    pub to: CemeteryZone,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FieldZone {
    Field,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayPermission {
    pub from: Vec<BanishPermissionZone>,
    pub on: BanishPermissionCards,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BanishPermissionZone {
    Banish,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BanishPermissionCards {
    pub who: SelfPlayer,
    pub zone: BanishPermissionZone,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Steal {
    pub r#ref: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Give {
    pub r#ref: String,
    pub to: GiveRecipient,
}

/// `other_players` are the players other than the giving one; any other name is one player.
pub(super) enum GiveRecipient {
    OtherPlayers,
    Player(Side),
}

impl<'de> Deserialize<'de> for GiveRecipient {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        Ok(if name == "other_players" {
            Self::OtherPlayers
        } else {
            Self::Player(Side::named(&name))
        })
    }
}
