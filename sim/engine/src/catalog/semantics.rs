//! Load-time semantic checks.
//!
//! The schema only says a `read`, event or selector type is a string; the engine then
//! resolves unknown names to null, 0 or "every card". Every such name is checked here
//! against what the engine actually implements, so a typo or an unimplemented feature
//! rejects the card at load instead of silently changing a result.

#![expect(
    clippy::indexing_slicing,
    reason = "Schema-validated JSON uses total read indexing."
)]

use alloc::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// One rejected construct: `needle` is the YAML text used to find its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) needle: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    Exec,
    Static,
    Cost,
    Construction,
}

/// Opcodes with an executor arm in `Game::execute` or a dedicated caller.
const EXEC_OPS: &[&str] = &[
    "seq",
    "if",
    "per",
    "repeat",
    "for_each",
    "choice",
    "declare_number",
    "optional",
    "pay",
    "if_done",
    "select",
    "damage",
    "destroy",
    "banish",
    "discard",
    "move",
    "transform",
    "evolve",
    "act",
    "stand",
    "draw",
    "random",
    "dice",
    "look",
    "search",
    "shuffle",
    "reveal",
    "modify",
    "pp",
    "recover_pp",
    "max_pp",
    "ep",
    "lesson",
    "eat",
    "race",
    "gain_drive",
    "stack",
    "drive",
    "skip_turn",
    "extra_turn",
    "win",
    "delay",
    "flip",
    "control",
    "create",
    "counter",
    "adjust_cost",
    "restrict",
    "replace_damage",
    "reveal_until",
    "box",
    "equip",
    "pilot",
    "become_type",
    "play_card",
    "play_ability",
];

/// Static bodies the engine reads (a static `seq` is split into one static per step).
const STATIC_OPS: &[&str] = &[
    "keyword",
    "aura",
    "adjust_cost",
    "restrict",
    "replace_damage",
    "allow_reroll",
    "rule_override",
    "play_permission",
    "replace_move",
    "repeat_triggers",
    "require_attack",
    "name_alias",
    "replace_choice",
];

/// Cost-only opcodes on top of the executable ones.
const COST_ONLY_OPS: &[&str] = &["earth_rite", "link_resource"];

/// Trigger events that some rule procedure actually collects.
const TRIGGER_EVENTS: &[&str] = &[
    "act",
    "stand",
    "attack",
    "damage",
    "deal_damage",
    "discard",
    "drive_trigger",
    "end",
    "enter",
    "evolve",
    "super_evolve",
    "ex_enter",
    "field_to_cemetery",
    "leave",
    "gain_drive",
    "hp_increase",
    "hp_decrease",
    "power_increase",
    "power_decrease",
    "leader_life_change",
    "main_start",
    "race",
    "targeted",
    "ub_activated",
    "banish",
    "card_play",
    "fusion",
];

/// Phase events carry no subject; `side` picks whose phase.
const PHASE_EVENTS: &[&str] = &["end", "main_start", "drive_trigger"];

const ABILITY_KINDS: &[&str] = &[
    "spell",
    "trigger",
    "static",
    "activated",
    "evolve",
    "construction",
    "play",
    "ride",
    "meal",
];

const ZONES: &[&str] = &[
    "field",
    "ex",
    "deck",
    "hand",
    "cemetery",
    "banish",
    "resolution",
    "evolve_deck",
    "evolution",
    "race",
    "drive",
    "trigger",
    "equipment",
];

const SELECT_TYPES: &[&str] = &["follower", "evolved_follower", "amulet", "spell", "crest"];

const SIDES: &[&str] = &["self", "opponent", "both", "active"];

/// Fields `event.*` can read, per trigger event (`frame.event`).
fn event_fields(event: &str) -> &'static [&'static str] {
    match event {
        "attack" => &["subject", "subject_id", "target", "target_is_follower"],
        "damage" => &["subject", "subject_id", "effect_damage"],
        "deal_damage" => &[
            "subject",
            "subject_id",
            "target",
            "amount",
            "battle",
            "to_opposing_leader",
        ],
        "leader_life_change" => &["subject", "subject_id", "before_life", "after_life"],
        "hp_increase" | "hp_decrease" | "power_increase" | "power_decrease" => {
            &["subject", "subject_id", "stat", "before", "after"]
        }
        "race" => &["subject", "subject_id", "n"],
        "fusion" => &["subject", "subject_id", "fused_trait"],
        "end" | "main_start" | "drive_trigger" => &[],
        _ => &["subject", "subject_id"],
    }
}

/// Per-player turn counters the engine increments (`Game::bump`).
const TURN_COUNTERS: &[&str] = &[
    "cards_played",
    "ub_activated",
    "attacks",
    "evolve_played",
    "evolutions",
    "discarded",
    "magic_item_banished",
    "leader_damaged",
    "leader_hp_decreased",
    "leader_hp_increased",
];

/// Object attributes `read_object_attribute` can return: derived values, object
/// state written by the engine, and printed face fields.
const OBJECT_FIELDS: &[&str] = &[
    "entered_by_play",
    "entered_by_effect",
    "zone",
    "class",
    "cost",
    "current_cost",
    "id",
    "token",
    "power",
    "hp",
    "max_hp",
    "acted",
    "evolved",
    "entered_this_turn",
    "entered_from",
    "entered_by",
    "face",
    "face_up",
    "damage",
    "silenced",
    "stats_increased_this_turn",
    "attacks_this_turn",
    "name",
    "traits",
    "card_class",
    "card_type",
];

/// Names the engine binds by itself.
const INTERNAL_BINDS: &[&str] = &[
    "item",
    "drive-target",
    "search-result",
    "created-tokens",
    "capacity-selected",
];

/// Restriction actions some rule procedure asks `Game::restricted` about.
const RESTRICTED_ACTIONS: &[&str] = &[
    "ability_destroy",
    "attack",
    "attack_leader",
    "banish",
    "draw",
    "ignore_guard",
    "lose",
    "normal_draw",
    "normal_max_pp_gain",
    "normal_stand",
    "play",
    "play_follower",
    "trigger",
    "win",
];

const CONTINUOUS_UNTIL: &[&str] = &[
    "game",
    "end-of-turn",
    "next-controller-end",
    "next-opponent-turn-end",
];

struct Checker<'registry> {
    keywords: &'registry BTreeMap<String, Value>,
    top: Vec<Value>,
    objects: BTreeSet<String>,
    values: BTreeSet<String>,
    findings: Vec<Finding>,
}

/// Checks one expanded card program.
pub(crate) fn check_program(program: &Value, keywords: &BTreeMap<String, Value>) -> Vec<Finding> {
    let mut checker = Checker {
        keywords,
        top: program["abilities"].as_array().cloned().unwrap_or_default(),
        objects: INTERNAL_BINDS
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        values: BTreeSet::new(),
        findings: Vec::new(),
    };
    checker.collect_binds(program);
    let abilities = checker.top.clone();
    checker.shared_lines(&abilities);
    for ability in &abilities {
        checker.ability(ability);
    }
    checker.findings
}

fn one_of(value: &str, allowed: &[&str]) -> bool {
    allowed.contains(&value)
}

impl Checker<'_> {
    fn reject(&mut self, needle: impl Into<String>, message: impl Into<String>) {
        self.findings.push(Finding {
            needle: needle.into(),
            message: message.into(),
        });
    }

    fn collect_binds(&mut self, value: &Value) {
        match value {
            Value::Object(fields) => {
                if let Some(bind) = fields.get("bind").and_then(Value::as_str) {
                    if matches!(
                        fields.get("op").and_then(Value::as_str),
                        Some("dice" | "declare_number")
                    ) {
                        self.values.insert(bind.to_owned());
                    } else {
                        self.objects.insert(bind.to_owned());
                    }
                }
                if let Some(costs) = fields.get("additional_costs").and_then(Value::as_array) {
                    for spec in costs {
                        if let Some(key) = spec["key"].as_str() {
                            self.values.insert(format!("paid.{key}"));
                        }
                    }
                }
                for member in fields.values() {
                    self.collect_binds(member);
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.collect_binds(item);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    /// Contract 3.1: abilities sharing a printed line are told apart only by keyword.
    /// Statics are never referenced by a decision, and entries with the same body are
    /// one printed ability split by trigger condition ("AときかBとき").
    fn shared_lines(&mut self, abilities: &[Value]) {
        let mut lines: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for ability in abilities
            .iter()
            .filter(|ability| ability["kind"] != "static")
        {
            let group = if ability["kind"] == "trigger" {
                "trigger"
            } else {
                "declared"
            };
            let key = format!(
                "{group} face {} section {} line {}",
                ability["face"], ability["section"], ability["line"]
            );
            lines.entry(key).or_default().push(ability);
        }
        for (key, entries) in lines {
            let bodies = entries
                .iter()
                .map(|ability| ability["body"].to_string())
                .collect::<BTreeSet<_>>();
            let labels = entries
                .iter()
                .map(|ability| {
                    ability["keyword"]
                        .as_str()
                        .or_else(|| ability["rule"].as_str())
                })
                .collect::<Vec<_>>();
            let distinct = labels.iter().flatten().collect::<BTreeSet<_>>();
            if bodies.len() > 1 && (labels.contains(&None) || distinct.len() != labels.len()) {
                self.reject(
                    "line:",
                    format!(
                        "{key}: abilities sharing a line need distinct keywords (contract 3.1)"
                    ),
                );
            }
        }
    }

    fn ability(&mut self, ability: &Value) {
        let kind = ability["kind"].as_str().unwrap_or_default();
        let event = ability["event"].as_str();
        self.trigger_fields(ability, kind, event);
        if let Some(keyword) = ability["keyword"].as_str()
            && !self.keywords.contains_key(keyword)
        {
            self.reject(
                format!("keyword: {keyword}"),
                format!("unknown keyword id `{keyword}`"),
            );
        }
        for zone in ability["active_zones"].as_array().into_iter().flatten() {
            let zone = zone.as_str().unwrap_or_default();
            if !one_of(zone, ZONES) {
                self.reject(zone, format!("unknown active zone `{zone}`"));
            }
        }
        if !one_of(kind, ABILITY_KINDS) {
            self.reject(
                format!("kind: {kind}"),
                format!("unknown ability kind `{kind}`"),
            );
        }
        let context = Context {
            event: if kind == "trigger" { event } else { None },
            variables: ability.get("variables").is_some(),
            choice: ability["body"]["op"] == "replace_choice",
        };
        self.ability_parts(ability, &context);
        let position = match kind {
            "static" => Position::Static,
            "construction" => Position::Construction,
            _ => Position::Exec,
        };
        if position == Position::Static && ability["body"]["op"] == "seq" {
            for step in ability["body"]["steps"].as_array().into_iter().flatten() {
                self.node(step, position, &context);
            }
        } else {
            self.node(&ability["body"], position, &context);
        }
    }

    fn trigger_fields(&mut self, ability: &Value, kind: &str, event: Option<&str>) {
        if kind != "trigger" {
            for key in ["event", "subject", "trigger_if", "side", "retain_event"] {
                if ability.get(key).is_some() {
                    self.reject(
                        format!("{key}:"),
                        format!("`{key}` is only read on trigger abilities"),
                    );
                }
            }
            return;
        }
        match event {
            Some(name) if one_of(name, TRIGGER_EVENTS) => {}
            Some(name) => self.reject(
                format!("event: {name}"),
                format!("trigger event `{name}` is never collected by the engine"),
            ),
            None => self.reject("kind: trigger", "trigger ability without event"),
        }
        let phase = event.is_some_and(|name| one_of(name, PHASE_EVENTS));
        if phase && ability.get("subject").is_some() {
            self.reject(
                "subject:",
                "phase events have no subject; it would be ignored",
            );
        }
        if !phase && ability.get("side").is_some() {
            self.reject(
                "side:",
                "trigger `side` only applies to phase events; it would be ignored",
            );
        }
    }

    fn ability_parts(&mut self, ability: &Value, context: &Context<'_>) {
        if let Some(subject) = ability.get("subject") {
            self.selector(subject, context);
        }
        for key in ["trigger_if", "play_if"] {
            if let Some(expr) = ability.get(key) {
                self.expr(expr, context);
            }
        }
        for key in ["targets", "cost_selections"] {
            for selection in ability[key].as_array().into_iter().flatten() {
                self.selection(selection, context);
            }
        }
        for cost in ability["costs"].as_array().into_iter().flatten() {
            self.node(cost, Position::Cost, context);
        }
        for spec in ability["additional_costs"].as_array().into_iter().flatten() {
            for cost in spec["costs"].as_array().into_iter().flatten() {
                self.node(cost, Position::Cost, context);
            }
            for selection in spec["cost_selections"].as_array().into_iter().flatten() {
                self.selection(selection, context);
            }
        }
        if let Some(bounds) = ability["variables"].get("x") {
            self.expr(&bounds["min"], context);
            self.expr(&bounds["max"], context);
        }
    }

    fn node(&mut self, node: &Value, position: Position, context: &Context<'_>) {
        let op = node["op"].as_str().unwrap_or_default();
        let allowed = match position {
            Position::Exec => one_of(op, EXEC_OPS),
            Position::Static => one_of(op, STATIC_OPS),
            Position::Cost => one_of(op, EXEC_OPS) || one_of(op, COST_ONLY_OPS),
            Position::Construction => op == "deck_limit",
        };
        if !allowed {
            self.reject(
                format!("op: {op}"),
                format!("opcode `{op}` is not implemented in {position:?} position"),
            );
        }
        self.parameters(node, op, context);
        for (key, value) in node.as_object().into_iter().flatten() {
            match key.as_str() {
                "steps" | "modes" => {
                    for step in value.as_array().into_iter().flatten() {
                        self.node(step, position.body(), context);
                    }
                }
                "body" | "attempt" | "then" | "else" => {
                    self.node(value, position.body(), context);
                }
                "costs" => {
                    for cost in value.as_array().into_iter().flatten() {
                        self.node(cost, Position::Cost, context);
                    }
                }
                "abilities" => {
                    let granted = value.as_array().cloned().unwrap_or_default();
                    // Abilities granted to the card itself share its printed line numbers.
                    let mut lines = granted.clone();
                    if node["subjects"] == "self" {
                        lines.extend(self.top.iter().cloned());
                    }
                    self.shared_lines(&lines);
                    for ability in &granted {
                        self.ability(ability);
                    }
                }
                "cost_selections" | "groups" => {
                    for selection in value.as_array().into_iter().flatten() {
                        self.selection(selection, context);
                    }
                }
                "selection" => self.selection(value, context),
                "select" | "subjects" | "sources" | "qualifies" | "left" | "right" => {
                    self.selector(value, context);
                }
                "tokens" => {
                    for token in value.as_array().into_iter().flatten() {
                        self.expr(&token["count"], context);
                    }
                }
                "until" if op == "repeat" => self.expr(value, context),
                "condition" | "count" | "amount" | "min" | "max" | "power" | "hp" | "set_power"
                | "set_hp" | "set_cost" | "cost" | "set" | "additional" | "distribute"
                | "constraint" => self.expr(value, context),
                _ => {}
            }
        }
    }

    fn parameters(&mut self, node: &Value, op: &str, context: &Context<'_>) {
        self.periods(node, op);
        self.event_parameters(node, op);
        self.value_parameters(node, op, context);
        self.names(node);
    }

    fn periods(&mut self, node: &Value, op: &str) {
        if let Some(side) = node["side"].as_str()
            && !one_of(side, SIDES)
        {
            self.reject(format!("side: {side}"), format!("unknown side `{side}`"));
        }
        if let Some(until) = node["until"].as_str() {
            let known = match op {
                // BOX lasts until its controller's next turn ends (apply_box).
                "box" => until == "next-controller-end",
                "delay" => until == "end-of-turn",
                _ => one_of(until, CONTINUOUS_UNTIL),
            };
            if !known {
                self.reject(
                    format!("until: {until}"),
                    format!("`{op}.until: {until}` has no expiry in the engine"),
                );
            }
        }
        if let Some(during) = node["during"].as_str() {
            let known = ["next-opponent-", "next-controller-"].iter().any(|prefix| {
                during
                    .strip_prefix(prefix)
                    .is_some_and(|phase| matches!(phase, "start" | "main" | "turn"))
            });
            if !known {
                self.reject(
                    format!("during: {during}"),
                    format!("unknown effect period `{during}`"),
                );
            }
        }
    }

    fn event_parameters(&mut self, node: &Value, op: &str) {
        match op {
            "delay" => {
                let event = node["event"].as_str().unwrap_or_default();
                if !matches!(event, "end" | "field_to_cemetery" | "leave") {
                    self.reject(
                        format!("event: {event}"),
                        format!("delayed event `{event}` is not implemented"),
                    );
                }
            }
            "play_ability" | "repeat_triggers" => {
                let event = node["event"].as_str().unwrap_or_default();
                if !one_of(event, TRIGGER_EVENTS) {
                    self.reject(
                        format!("event: {event}"),
                        format!("unknown event `{event}` in `{op}`"),
                    );
                }
            }
            "restrict" => {
                let action = node["action"].as_str().unwrap_or_default();
                if !one_of(action, RESTRICTED_ACTIONS) {
                    self.reject(
                        format!("action: {action}"),
                        format!("no rule procedure checks the restriction `{action}`"),
                    );
                }
                for event in node["events"].as_array().into_iter().flatten() {
                    let event = event.as_str().unwrap_or_default();
                    if !matches!(event, "fanfare" | "on_evolve") {
                        self.reject(
                            event,
                            format!("trigger suppression `{event}` is not implemented"),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn value_parameters(&mut self, node: &Value, op: &str, context: &Context<'_>) {
        match op {
            "draw" | "look" if node.get("up_to").is_some() => {
                self.reject("up_to:", format!("`{op}.up_to` is not implemented"));
            }
            "modify" if context.event.is_some() || node.get("abilities").is_none() => {
                for field in ["type", "traits", "cost", "set_cost"] {
                    if node.get(field).is_some() {
                        self.reject(
                            format!("{field}:"),
                            format!("`modify.{field}` is declarative-only and not executable"),
                        );
                    }
                }
            }
            "adjust_cost" => {
                if let Some(consume) = node["consume_on"].as_str() {
                    // The engine consumes a limited reduction on the play it applies to,
                    // which is what these three spellings mean; anything else is unknown.
                    let equivalent = matches!(consume, "play" | "card_play" | "spell_play")
                        && node.get("uses").is_some()
                        && node["kind"] != "evolve";
                    if !equivalent {
                        self.reject(
                            format!("consume_on: {consume}"),
                            format!("`consume_on: {consume}` is not implemented"),
                        );
                    }
                }
            }
            "counter" => {
                let name = node["name"].as_str().unwrap_or_default();
                if self
                    .keywords
                    .get(name)
                    .is_none_or(|entry| entry["role"] != "counter")
                {
                    self.reject(
                        format!("name: {name}"),
                        format!("counter `{name}` is not a registered counter id"),
                    );
                }
            }
            "name_alias" if !matches!(node["while_zone"].as_str(), Some("any" | "field")) => {
                self.reject("while_zone:", "name alias zone must be `any` or `field`");
            }
            _ => {}
        }
    }

    fn names(&mut self, node: &Value) {
        for key in ["keywords"] {
            for keyword in node[key].as_array().into_iter().flatten() {
                let keyword = keyword.as_str().unwrap_or_default();
                if !self.keywords.contains_key(keyword) {
                    self.reject(keyword, format!("unknown keyword id `{keyword}`"));
                }
            }
        }
        for key in ["to", "from_zone", "while_zone"] {
            if let Some(zone) = node[key].as_str()
                && !one_of(zone, ZONES)
                && !matches!(zone, "any" | "all" | "self" | "opponent")
            {
                self.reject(format!("{key}: {zone}"), format!("unknown zone `{zone}`"));
            }
        }
    }

    fn selection(&mut self, selection: &Value, context: &Context<'_>) {
        self.selector(&selection["select"], context);
        for key in ["min", "max", "distribute", "constraint"] {
            if let Some(expr) = selection.get(key) {
                self.expr(expr, context);
            }
        }
        if let Some(by) = selection["distinct_by"].as_str()
            && by != "name"
        {
            self.reject(
                format!("distinct_by: {by}"),
                format!("`distinct_by: {by}` is not implemented"),
            );
        }
    }

    fn selector(&mut self, selector: &Value, context: &Context<'_>) {
        match selector {
            Value::String(reference) => self.reference(reference),
            Value::Object(fields) => {
                for key in ["union", "difference"] {
                    for part in fields
                        .get(key)
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        self.selector(part, context);
                    }
                }
                if let Some(source) = fields.get("from") {
                    self.selector(source, context);
                }
                if let Some(typ) = fields.get("type").and_then(Value::as_str)
                    && !one_of(typ, SELECT_TYPES)
                {
                    self.reject(format!("type: {typ}"), format!("unknown card type `{typ}`"));
                }
                if let Some(keyword) = fields.get("keyword").and_then(Value::as_str)
                    && !self.keywords.contains_key(keyword)
                {
                    self.reject(
                        format!("keyword: {keyword}"),
                        format!("unknown keyword id `{keyword}`"),
                    );
                }
                if let Some(event) = fields.get("ability_event").and_then(Value::as_str)
                    && !one_of(event, ABILITY_KINDS)
                    && !one_of(event, TRIGGER_EVENTS)
                {
                    self.reject(event, format!("unknown ability event `{event}`"));
                }
                for key in ["top", "where"] {
                    if let Some(expr) = fields.get(key) {
                        self.expr(expr, context);
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) => {}
        }
    }

    fn reference(&mut self, reference: &str) {
        let known = matches!(
            reference,
            "self"
                | "self.leader"
                | "opponent.leader"
                | "both.leaders"
                | "linked"
                | "event.target"
                | "event.subject"
        ) || reference.strip_prefix("target.").is_some_and(|rest| {
            let index = rest.strip_suffix(".leader").unwrap_or(rest);
            !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit())
        }) || reference.starts_with("cost.")
            || {
                let base = reference.strip_suffix(".leader").unwrap_or(reference);
                self.objects.contains(base)
            };
        if !known {
            self.reject(reference, format!("unknown reference `{reference}`"));
        }
    }

    fn expr(&mut self, expr: &Value, context: &Context<'_>) {
        match expr {
            Value::Object(fields) => {
                if let Some(path) = fields.get("read").and_then(Value::as_str)
                    && let Err(message) = self.read(path, context)
                {
                    self.reject(format!("read: {path}"), message);
                }
                if let Some(selector) = fields.get("count") {
                    self.selector(selector, context);
                }
                if let Some(selector) = fields.get("values") {
                    self.selector(selector, context);
                    let field = fields
                        .get("field")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !one_of(field, OBJECT_FIELDS) {
                        self.reject(
                            format!("field: {field}"),
                            format!("unknown object field `{field}`"),
                        );
                    }
                }
                for key in ["if", "then", "else", "value"] {
                    if let Some(inner) = fields.get(key) {
                        self.expr(inner, context);
                    }
                }
                for arg in fields
                    .get("args")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    self.expr(arg, context);
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.expr(item, context);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    fn read(&self, path: &str, context: &Context<'_>) -> Result<(), String> {
        // `paid.<key>` is set by an additional cost of any ability of the same card
        // (a static cost reduction reads the play ability's payment).
        if self.values.contains(path) {
            return Ok(());
        }
        if path == "x" {
            return if context.variables {
                Ok(())
            } else {
                Err("`x` needs `variables.x` on the same ability".into())
            };
        }
        if matches!(path, "turn.phase" | "damage.amount") {
            return Ok(());
        }
        if matches!(path, "choice.min" | "choice.max" | "choice.mode_count") {
            return if context.choice {
                Ok(())
            } else {
                Err(format!("`{path}` is only set inside `replace_choice`"))
            };
        }
        if let Some(rest) = path.strip_prefix("event.") {
            let event = context
                .event
                .ok_or_else(|| format!("`{path}` read outside a trigger"))?;
            let (field, attribute) = rest.split_once('.').unwrap_or((rest, ""));
            if !event_fields(event).contains(&field) {
                return Err(format!("event `{event}` has no field `{field}`"));
            }
            if field == "subject" && !attribute.is_empty() && !one_of(attribute, OBJECT_FIELDS) {
                return Err(format!("unknown subject attribute `{attribute}`"));
            }
            return Ok(());
        }
        if let Some(counter) = path.strip_prefix("self.counters.") {
            return if self
                .keywords
                .get(counter)
                .is_some_and(|entry| entry["role"] == "counter")
            {
                Ok(())
            } else {
                Err(format!("`{counter}` is not a registered counter id"))
            };
        }
        if let Some(rest) = path
            .strip_prefix("self.")
            .or_else(|| path.strip_prefix("opponent."))
        {
            if matches!(
                rest,
                "is_active" | "combo" | "pp.current" | "pp.max" | "leader.life" | "cemetery.cost"
            ) {
                return Ok(());
            }
            if let Some(counter) = rest.strip_prefix("turn.") {
                let known = one_of(counter, TURN_COUNTERS)
                    || counter
                        .strip_prefix("trait_attacks.")
                        .is_some_and(|name| !name.is_empty());
                return if known {
                    Ok(())
                } else {
                    Err(format!("turn counter `{counter}` is never counted"))
                };
            }
        }
        for role in ["controller", "owner"] {
            if let Some((reference, property)) = path.split_once(&format!(".{role}.")) {
                let zone = property.strip_suffix("_count").unwrap_or_default();
                if !matches!(
                    zone,
                    "field" | "hand" | "deck" | "ex" | "cemetery" | "banish" | "evolve_deck"
                ) {
                    return Err(format!("unknown player property `{property}`"));
                }
                return self.object_reference(reference);
            }
        }
        let (reference, field) = path
            .rsplit_once('.')
            .ok_or_else(|| format!("unknown read `{path}`"))?;
        self.object_reference(reference)?;
        if field == "original_hp" && !reference.starts_with("target.") {
            return Err("`original_hp` is only captured for play-time targets".into());
        }
        if one_of(field, OBJECT_FIELDS) || field == "original_hp" {
            Ok(())
        } else {
            Err(format!("unknown object field `{field}`"))
        }
    }

    fn object_reference(&self, reference: &str) -> Result<(), String> {
        let known = matches!(
            reference,
            "self" | "item" | "event.target" | "event.subject"
        ) || reference
            .strip_prefix("target.")
            .is_some_and(|index| index.bytes().all(|b| b.is_ascii_digit()))
            || reference.starts_with("cost.")
            || self.objects.contains(reference);
        if known {
            Ok(())
        } else {
            Err(format!("unknown reference `{reference}`"))
        }
    }
}

struct Context<'event> {
    event: Option<&'event str>,
    variables: bool,
    /// Inside `replace_choice`, which reads the bounds it replaces.
    choice: bool,
}

impl Position {
    const fn body(self) -> Self {
        match self {
            Self::Cost | Self::Exec => Self::Exec,
            Self::Static => Self::Static,
            Self::Construction => Self::Construction,
        }
    }
}
