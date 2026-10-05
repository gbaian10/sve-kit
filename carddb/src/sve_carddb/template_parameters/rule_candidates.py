"""Closed, opt-in recognition proposals; these definitions grant no adoption authority."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.explicit_rules import EXPLICIT
from sve_carddb.template_parameters.keyword_aliases import KEYWORD_ALIASES
from sve_carddb.template_parameters.numeric_rules import (
    ASCII_AFTER,
    ASCII_BEFORE,
    SIGNS,
)
from sve_carddb.template_parameters.signed_contexts import SIGNED_CONTEXTS

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
KEYWORD_PATTERN = r"【([^【】]+)_$"
MIN_OPTIONS = 2
STAT_CATEGORIES = {"term:stat.attack": "rule_term", "term:stat.health": "rule_term"}


@dataclass(frozen=True)
class Rule:
    id: str
    role: str
    reason: str
    condition: str
    targets: tuple[str, ...] = ()


RULES: tuple[Rule, ...] = (
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
        "adjacent braced attack marker immediately followed by ASCII sign and unsigned magnitude; no separator",
        ("term:stat.attack",),
    ),
    Rule(
        "prefix_health_delta",
        "health_delta_magnitude",
        "signed_numeric_requires_review",
        "adjacent braced health marker immediately followed by ASCII sign and unsigned magnitude; no separator",
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
        "named ability prefix with ASCII underscore, unsigned threshold and adjacent closing bracket",
        ("term:ability.combo",),
    ),
    Rule(
        "keyword_threshold_lesson",
        "lesson_threshold",
        "numeric_identifier_requires_review",
        "named ability prefix with ASCII underscore, unsigned threshold and adjacent closing bracket",
        ("term:ability.lesson",),
    ),
    Rule(
        "keyword_threshold_necrocharge",
        "necrocharge_threshold",
        "numeric_identifier_requires_review",
        "named ability prefix with ASCII underscore, unsigned threshold and adjacent closing bracket",
        ("term:ability.necrocharge",),
    ),
    Rule(
        "keyword_threshold_spell_chain",
        "spell_chain_threshold",
        "numeric_identifier_requires_review",
        "named ability prefix with ASCII underscore, unsigned threshold and adjacent closing bracket",
        ("term:ability.spell_chain",),
    ),
    Rule(
        "braced_stat_reference",
        "stat_marker_reference",
        "term_role_requires_review",
        "adjacent braces with balanced left prefix around exact unique adopted stat name",
        ("term:stat.attack", "term:stat.health"),
    ),
    Rule(
        "braced_ability_reference",
        "ability_marker_reference",
        "term_role_requires_review",
        "adjacent braces with balanced left prefix around exact unique adopted ability name in closed set",
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
        "adjacent braces with balanced left prefix around exact unique adopted engage name; category is verified from record",
        ("term:action.engage",),
    ),
)
RULES += tuple(
    Rule(
        identifier,
        spec.role,
        "numeric_role_requires_review",
        "finite explicit prefix and suffix",
    )
    for identifier, spec in EXPLICIT.items()
)
RULES += tuple(
    Rule(
        identifier,
        spec.role,
        "signed_numeric_requires_review",
        "finite signed context",
        (spec.target,) if spec.target else (),
    )
    for identifier, spec in SIGNED_CONTEXTS.items()
)
RULES += tuple(
    Rule(
        identifier,
        alias.role,
        "numeric_identifier_requires_review",
        "closed raw alias and unique registered full ability",
        (alias.target,),
    )
    for identifier, alias in KEYWORD_ALIASES.items()
)
BY_ID = {rule.id: rule for rule in RULES}


def selection(enabled: tuple[str, ...]) -> tuple[str, ...]:
    """An explicit switch selects proposals, never a policy or an implicit whole family."""
    if len(enabled) != len(set(enabled)):
        raise ValueError("Candidate rule selection must be unique")
    if any(rule not in BY_ID for rule in enabled):
        raise ValueError("Candidate rule selection contains an unknown rule")
    return tuple(sorted(enabled))


def _explicit_conditions(identifier: str) -> dict[str, JsonValue]:
    spec = EXPLICIT[identifier]
    result: dict[str, JsonValue] = {
        "explicit_evidence": {
            "prefix_pattern": spec.before,
            "suffix_pattern": spec.after,
            "minimum": spec.minimum,
            "maximum": 9007199254740991,
            "raw_unsigned_decimal": True,
            "context_includes_numeric_span": True,
        }
    }
    if spec.companion is not None:
        result["raw_safe_unsigned_companion"] = spec.companion
    return result


def conditions(rule: Rule) -> dict[str, JsonValue]:
    """Only matching inputs and role evidence belong to the maintainer condition hash."""
    result: dict[str, JsonValue] = {
        "required_issue": rule.reason,
        "proposed_role": rule.role,
        "scope": {
            "region": "jp",
            "roles": ["body"]
            if rule.id == "bracket_choice_index"
            else ["body", "reminder"],
        },
        "context": {"body_nfkc": True, "reminder_nfkc": False},
        "existing_numeric_rule": False,
        "invalid_safe_unsigned_decimal": False,
    }
    if rule.id in EXPLICIT:
        result.update(_explicit_conditions(rule.id))
        return result
    if rule.id in KEYWORD_ALIASES:
        alias = KEYWORD_ALIASES[rule.id]
        result["keyword_alias_evidence"] = {
            "raw_prefix": "【" + alias.spelling + "_",
            "raw_exact": True,
            "full_registered_name": alias.full_name,
            "target_id": alias.target,
            "unique_ability": True,
            "adjacent_closing_bracket": True,
            "minimum": 0,
            "maximum": 9007199254740991,
        }
        return result
    if rule.id in SIGNED_CONTEXTS:
        signed_spec = SIGNED_CONTEXTS[rule.id]
        result["signed_context_evidence"] = {
            "prefix_pattern": signed_spec.before,
            "suffix_pattern": signed_spec.after,
            "minimum": 0,
            "maximum": 9007199254740991,
            "raw_unsigned_magnitude": True,
            "sign_is_literal": True,
            "context_includes_magnitude_and_suffix": True,
            "exact_unique_ability": signed_spec.target,
        }
        return result
    if rule.targets:
        result["reference_evidence"] = {
            "raw_name_exact": True,
            "adopted_count": 1,
            "targets": list(rule.targets),
            "categories": {
                target: STAT_CATEGORIES.get(target, "ability")
                for target in rule.targets
            },
            "record_hash": True,
        }
    if rule.id.startswith("braced_"):
        result["braced_evidence"] = {
            "before_ends_with": "{",
            "after_starts_with": "}",
            "left_prefix_brace_balance": 1,
            "hint_target_equals_adopted": True,
        }
        return result
    result["exact_raw_safe_unsigned_decimal_equals_value"] = True
    if rule.id in SUFFIXES:
        result["suffix_pattern"] = SUFFIXES[rule.id]
        result["preceding_sign_exclusion"] = list(SIGNS)
        result["ascii_before_exclusion"] = ASCII_BEFORE
        if rule.id.startswith("suffix_ordinal_"):
            result["zero_excluded"] = True
    elif rule.id in SIGNED:
        result["prefix_pattern"] = SIGNED[rule.id]
        result["ascii_after_exclusion"] = ASCII_AFTER
        result["sign_remains_literal"] = True
    elif rule.id.startswith("keyword_threshold_"):
        result["keyword_pattern"] = KEYWORD_PATTERN
        result["after_starts_with"] = "】"
        result["name_remains_literal"] = True
    else:
        result["choice_evidence"] = {
            "before_ends_with": "【",
            "after_starts_with": "】",
            "introduction_pattern": INTRO_PATTERN,
            "label_pattern": LABEL_PATTERN,
            "minimum_options": MIN_OPTIONS,
            "same_full_field": True,
            "body_only": True,
            "quoted_spans_excluded": True,
            "introduction_precedes_labels": True,
            "stop_at_next_introduction": True,
            "ordered_indices": "1..k",
            "slot_is_exact_label_digits": True,
        }
    return result


def condition_hash(rule: Rule) -> str:
    """Descriptions, publication status and matcher version do not alter lexical conditions."""
    return digest(canonical(conditions(rule)))


def definition(rule: Rule) -> dict[str, JsonValue]:
    """Keep display metadata separate from the exact matching contract."""
    return {
        "id": rule.id,
        "matcher_version": VERSION + ":" + rule.id,
        "proposed_role": rule.role,
        "original_reason": rule.reason,
        "condition": rule.condition,
        "match_conditions": conditions(rule),
        "condition_hash": condition_hash(rule),
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
        "rules": [definition(rule) for rule in RULES],
    }
