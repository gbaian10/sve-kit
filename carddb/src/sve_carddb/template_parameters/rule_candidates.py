"""Closed, opt-in recognition proposals; these definitions grant no adoption authority."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from pydantic import JsonValue

VERSION = "parameter-rule-candidates-v1"
SUFFIXES = {
    "suffix_damage_amount": r"^ダメージ(?![A-Za-z0-9_])",
    "suffix_recovery_amount": r"^回復(?![A-Za-z0-9_])",
    "suffix_ordinal_cards": r"^枚目(?![A-Za-z0-9_])",
    "suffix_ordinal_times": r"^回目(?![A-Za-z0-9_])",
    "suffix_ordinal_turns": r"^ターン目(?![A-Za-z0-9_])",
    "suffix_unit_items": r"^つ(?:チョイス|まで|を|持つ|裏向き|[）)]|以上|につ|の|。)",
}
SIGNED = {
    "prefix_attack_delta": r"\{攻撃力\}[+-]$",
    "prefix_health_delta": r"\{体力\}[+-]$",
    "prefix_cost_delta": r"コストを[+-]$",
}
INTRO_PATTERN = r"(?<![A-Za-z0-9_０-９])[0-9０-９]+つ(?:まで)?チョイス"
LABEL_PATTERN = r"【([0-9０-９]+)】"


@dataclass(frozen=True)
class Rule:
    id: str
    role: str
    reason: str
    condition: str
    targets: tuple[str, ...] = ()


RULES = (
    Rule(
        "suffix_damage_amount",
        "damage_amount",
        "numeric_role_requires_review",
        "immediate damage suffix; no ASCII identifier or sign",
    ),
    Rule(
        "suffix_recovery_amount",
        "recovery_amount",
        "numeric_recovery_amount_requires_review",
        "immediate recovery suffix; no ASCII identifier or sign",
    ),
    Rule(
        "suffix_ordinal_cards",
        "card_ordinal",
        "numeric_ordinal_requires_review",
        "immediate cards unit plus ordinal marker; positive unsigned index",
    ),
    Rule(
        "suffix_ordinal_times",
        "repetition_ordinal",
        "numeric_ordinal_requires_review",
        "immediate times unit plus ordinal marker; positive unsigned index",
    ),
    Rule(
        "suffix_ordinal_turns",
        "turn_ordinal",
        "numeric_ordinal_requires_review",
        "immediate turns unit plus ordinal marker; positive unsigned index",
    ),
    Rule(
        "suffix_unit_items",
        "item_quantity",
        "numeric_role_requires_review",
        "immediate generic item unit with closed continuation set; no sign or ASCII identifier",
    ),
    Rule(
        "bracket_choice_index",
        "choice_ordinal",
        "numeric_role_requires_review",
        "bare bracketed index; earlier same-field numeric choice introduction and contiguous 1..k option sequence, k >= 2",
    ),
    Rule(
        "prefix_attack_delta",
        "attack_delta_magnitude",
        "signed_numeric_requires_review",
        "complete braced attack marker immediately followed by ASCII sign and unsigned magnitude; no separator",
        ("term:stat.attack",),
    ),
    Rule(
        "prefix_health_delta",
        "health_delta_magnitude",
        "signed_numeric_requires_review",
        "complete braced health marker immediately followed by ASCII sign and unsigned magnitude; no separator",
        ("term:stat.health",),
    ),
    Rule(
        "prefix_cost_delta",
        "cost_delta_magnitude",
        "signed_numeric_requires_review",
        "cost plus object particle immediately followed by ASCII sign and unsigned magnitude",
    ),
    Rule(
        "keyword_threshold_combo",
        "combo_threshold",
        "numeric_identifier_requires_review",
        "complete named ability bracket with ASCII underscore and unsigned threshold",
        ("term:ability.combo",),
    ),
    Rule(
        "keyword_threshold_lesson",
        "lesson_threshold",
        "numeric_identifier_requires_review",
        "complete named ability bracket with ASCII underscore and unsigned threshold",
        ("term:ability.lesson",),
    ),
    Rule(
        "keyword_threshold_necrocharge",
        "necrocharge_threshold",
        "numeric_identifier_requires_review",
        "complete named ability bracket with ASCII underscore and unsigned threshold",
        ("term:ability.necrocharge",),
    ),
    Rule(
        "keyword_threshold_spell_chain",
        "spell_chain_threshold",
        "numeric_identifier_requires_review",
        "complete named ability bracket with ASCII underscore and unsigned threshold",
        ("term:ability.spell_chain",),
    ),
    Rule(
        "braced_stat_reference",
        "stat_marker_reference",
        "term_role_requires_review",
        "complete braces around exact unique adopted stat name",
        ("term:stat.attack", "term:stat.health"),
    ),
    Rule(
        "braced_ability_reference",
        "ability_marker_reference",
        "term_role_requires_review",
        "complete braces around exact unique adopted ability name in closed set",
        (
            "term:ability.fanfare",
            "term:ability.activation",
            "term:ability.last_words",
            "term:ability.feed",
            "term:ability.union_burst",
            "term:ability.quick",
            "term:ability.possession",
            "term:ability.advance_activation",
        ),
    ),
    Rule(
        "braced_action_engage_reference",
        "action_marker_reference",
        "term_role_requires_review",
        "complete braces around exact unique adopted engage name; category is verified from record",
        ("term:action.engage",),
    ),
)
BY_ID = {rule.id: rule for rule in RULES}


def selection(enabled: tuple[str, ...]) -> tuple[str, ...]:
    """An explicit switch selects proposals, never a policy or an implicit whole family."""
    if len(enabled) != len(set(enabled)):
        raise ValueError("Candidate rule selection must be unique")
    if any(rule not in BY_ID for rule in enabled):
        raise ValueError("Candidate rule selection contains an unknown rule")
    return tuple(sorted(enabled))


def definition(rule: Rule) -> dict[str, JsonValue]:
    """Condition hashes bind exact syntax as well as the non-regex evidence requirements."""
    return {
        "id": rule.id,
        "matcher_version": VERSION + ":" + rule.id,
        "proposed_role": rule.role,
        "original_reason": rule.reason,
        "condition": rule.condition,
        "suffix_pattern": SUFFIXES.get(rule.id),
        "prefix_pattern": SIGNED.get(rule.id),
        "keyword_pattern": r"【([^【】]+)_$"
        if rule.id.startswith("keyword_threshold_")
        else None,
        "choice_introduction": INTRO_PATTERN
        if rule.id == "bracket_choice_index"
        else None,
        "choice_label": LABEL_PATTERN if rule.id == "bracket_choice_index" else None,
        "targets": list(rule.targets),
        "scope": {"region": "jp", "roles": ["body", "reminder"]},
        "ownership": "only pending original_reason slots; never any existing numeric_rule",
        "decimal_evidence": "exact ASCII/fullwidth decimal source spelling, safe unsigned value; ordinal zero excluded",
        "suffix_guards": "no preceding +,-,U+2212,fullwidth plus/minus or ASCII letter/digit/underscore",
        "signs": "ASCII plus/minus after original body NFKC or unchanged reminder; sign remains literal; no optional separator",
        "reference_evidence": "exact raw name, one adopted concept, closed ID set, record hash; rule_term for stats and ability for all others",
        "braced_shape": "complete nonnested left/right braces; name-only slot and exact target kind/id/hash agree",
        "keyword_shape": "complete left bracket, exact name, ASCII underscore, unsigned threshold, right bracket; name remains literal",
        "choice_evidence": "same full field body spans only; earlier introduction, complete contiguous 1..k label group with at least two items; slot covers only label digits",
        "status": "pending_approval",
    }


def configuration(enabled: tuple[str, ...] = ()) -> dict[str, JsonValue]:
    """Pin the entire closed registry and the exact switch set in the recipe config."""
    selected = selection(enabled)
    return {
        "version": VERSION,
        "enabled": list(selected),
        "recognition_policy": None,
        "status": "pending_approval",
        "region": "jp",
        "roles": ["body", "reminder"],
        "rules": [
            {**definition(rule), "condition_hash": digest(canonical(definition(rule)))}
            for rule in RULES
        ],
    }
