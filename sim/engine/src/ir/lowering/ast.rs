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
    pub define: BTreeMap<String, Number>,
    #[serde(rename = "do")]
    pub steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Step {
    Draw(Draw),
    Damage(Damage),
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Recipients {
    pub leaders: AllPlayers,
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
