"""Fixed synthetic full-field evidence and an adapter that recomputes every hint."""

from functools import cache
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameter_rules.models import (
    LEGACY_IDS,
    CaseInput,
    Example,
    Examples,
    Reference,
    RoleRange,
)
from sve_carddb.template_parameters.analysis import analyze
from sve_carddb.template_parameters.candidate_matching import recognize
from sve_carddb.template_parameters.models import Range
from sve_carddb.template_parameters.references import References
from sve_carddb.template_parameters.rule_candidates import BY_ID, STAT_CATEGORIES
from sve_carddb.template_parameters.spans import locate
from sve_carddb.template_sources.inventory import entry
from sve_carddb.template_sources.normalizer import VERSION, partition
from sve_carddb.template_sources.pins import PARSER

if TYPE_CHECKING:
    from sve_carddb.template_parameters.models import Candidate, Hint
    from sve_carddb.template_sources.normalizer import Part

HASH = digest(b"Synthetic adopted concept")
SUFFIX = {
    "suffix_unit_cards": "枚",
    "suffix_unit_entities": "体",
    "suffix_unit_times": "回",
    "suffix_unit_turns": "ターン",
    "suffix_unit_pp": "PP",
    "suffix_damage_amount": "ダメージ",
    "suffix_recovery_amount": "回復",
    "suffix_ordinal_cards": "枚目",
    "suffix_ordinal_times": "回目",
    "suffix_ordinal_turns": "ターン目",
    "suffix_unit_items": "つを試す",
}
PREFIX = {
    "prefix_field_attack": "攻撃力",
    "prefix_field_cost": "コスト",
    "prefix_field_health": "体力",
    "prefix_attack_delta": "{攻撃力}+",
    "prefix_health_delta": "{体力}+",
    "prefix_cost_delta": "コストを+",
}


def references(values: tuple[Reference, ...]) -> References:
    """Synthetic declarations never load a real glossary or authorize source concepts."""
    result = References()
    for item in values:
        result.terms.setdefault(item.source_name, []).append(
            (item.id, item.category, item.record_hash)
        )
    return result


def evidence(
    text: str, spans: tuple[Range, ...], refs: References
) -> tuple[tuple[RoleRange, ...], Part | None, Candidate | None, Hint | None]:
    """Bind the selected raw position to its complete recomputed source partition."""
    parts = partition(text)
    roles = tuple(
        sorted(
            (
                RoleRange(role=p.role, start=s.start, end=s.end)
                for p in parts
                for s in p.segments
            ),
            key=lambda s: s.start,
        )
    )
    source = SourceRef(
        store_id="synthetic",
        batch_id=HASH,
        source_version_id="src:v1:" + HASH[7:],
        parser=PARSER,
        locator="/faces/0/text",
        text_hash=digest(text.encode()),
    )
    for part, position in zip(parts, locate(text, parts), strict=True):
        if not all(
            any(s.start <= span.start < span.end <= s.end for s in part.segments)
            for span in spans
        ):
            continue
        candidate = analyze(text, part, entry(source, part, VERSION), position, refs)
        found = [h for h in candidate.slots if h.source_segments == spans]
        return roles, part, candidate, found[0] if len(found) == 1 else None
    return roles, None, None, None


def evaluate(rule_id: str, case: CaseInput) -> bool:
    """Declared roles and causes are assertions, not trusted matcher inputs."""
    refs = references(case.reference_candidates)
    roles, part, candidate, hint = evidence(case.field_text, case.slot_segments, refs)
    if roles != case.role_spans:
        raise ValueError("Recognition case partition differs from recomputed roles")
    if part is None or part.role != case.role:
        raise ValueError("Recognition case selected position has no matching role")
    actual_issues = tuple(sorted(hint.issues)) if hint is not None else ()
    numeric = hint.numeric_rule if hint is not None else None
    if actual_issues != case.original_issues or numeric != case.existing_numeric_rule:
        raise ValueError("Recognition case causes or old ownership differ from replay")
    if case.region != "jp" or part.role not in {"body", "reminder"} or hint is None:
        return False
    if rule_id in LEGACY_IDS:
        return numeric == rule_id and hint.value is not None
    assert candidate is not None
    return any(
        row["source_segments"]
        == [span.model_dump(mode="json") for span in case.slot_segments]
        for row in recognize(case.field_text, part, candidate, refs, (rule_id,))
    )


def _example(
    case_id: str,
    text: str,
    raw: str,
    expected: bool,
    refs: tuple[Reference, ...],
    *,
    region: str = "jp",
) -> Example:
    start = text.index(raw)
    spans = (Range(start=start, end=start + len(raw)),)
    roles, part, _candidate, hint = evidence(text, spans, references(refs))
    assert part is not None
    # JSON validation permits tuples while preserving strict scalar types.
    return Example.model_validate_json(
        canonical(
            {
                "case_id": case_id,
                "expected_match": expected,
                "input": {
                    "region": region,
                    "role": part.role,
                    "field_text": text,
                    "role_spans": [s.model_dump(mode="json") for s in roles],
                    "slot_segments": [s.model_dump(mode="json") for s in spans],
                    "original_issues": list[JsonValue](sorted(hint.issues))
                    if hint
                    else [],
                    "existing_numeric_rule": hint.numeric_rule if hint else None,
                    "reference_candidates": [r.model_dump(mode="json") for r in refs],
                },
            }
        )
    )


@cache
def fixed_examples(rule_id: str) -> Examples:
    """Every registered matcher has independently chosen positive and boundary cases."""
    refs: tuple[Reference, ...] = ()
    if rule_id in BY_ID and BY_ID[rule_id].targets:
        target = BY_ID[rule_id].targets[0]
        name = (
            "攻撃力"
            if target == "term:stat.attack"
            else "体力"
            if target == "term:stat.health"
            else "SyntheticTerm"
        )
        refs = (
            Reference(
                source_name=name,
                id=target,
                category=STAT_CATEGORIES.get(target, "ability"),
                record_hash=HASH,
            ),
        )
    if rule_id in SUFFIX:
        positive = "試験2" + SUFFIX[rule_id]
        negative = positive.replace("2", "+2", 1)
    elif rule_id in PREFIX:
        positive = PREFIX[rule_id] + "2試験"
        negative = "未知+2試験"
    elif rule_id.startswith("keyword_threshold_"):
        positive = "【SyntheticTerm_2】"
        negative = "【OtherTerm_2】"
    elif rule_id.startswith("braced_"):
        positive = "{" + refs[0].source_name + "}試験"
        negative = "{" + refs[0].source_name + "}試験"
    elif rule_id == "bracket_choice_index":
        positive = "3つチョイス\n【1】試験\n【2】試験"
        negative = "（3つチョイス）\n【1】試験\n【2】試験"
    else:
        raise ValueError("Unsupported recognition rule")
    raw = (
        refs[0].source_name
        if rule_id.startswith("braced_")
        else "1"
        if rule_id == "bracket_choice_index"
        else "2"
    )
    positive_cases = [_example("body_positive", positive, raw, True, refs)]
    negative_cases = [
        _example(
            "lexical_negative",
            negative,
            raw,
            False,
            () if rule_id.startswith("braced_") else refs,
        ),
        _example("region_negative", positive, raw, False, refs, region="en"),
    ]
    if rule_id != "bracket_choice_index":
        positive_cases.append(
            _example("reminder_positive", "（" + positive + "）", raw, True, refs)
        )
    if not rule_id.startswith("braced_"):
        invalid = positive.replace(raw, "9007199254740992", 1)
        negative_cases.append(
            _example("overflow_negative", invalid, "9007199254740992", False, refs)
        )
    _boundaries(rule_id, positive, raw, refs, positive_cases, negative_cases)
    return Examples(positive=tuple(positive_cases), negative=tuple(negative_cases))


def _boundaries(
    rule_id: str,
    positive: str,
    raw: str,
    refs: tuple[Reference, ...],
    positive_cases: list[Example],
    negative_cases: list[Example],
) -> None:
    if rule_id in SUFFIX and rule_id != "suffix_unit_items":
        negative_cases.append(
            _example("ascii_tail_negative", positive + "A", raw, False, refs)
        )
    if rule_id in LEGACY_IDS or rule_id in SUFFIX:
        sign_text = "（" + positive.replace(raw, "＋" + raw, 1) + "）"
        negative_cases.append(
            _example("reminder_fullwidth_sign_negative", sign_text, raw, False, refs)
        )
    if rule_id in {"prefix_attack_delta", "prefix_health_delta", "prefix_cost_delta"}:
        negative_cases.append(
            _example(
                "reminder_fullwidth_sign_negative",
                "（" + positive.replace("+", "＋") + "）",
                raw,
                False,
                refs,
            )
        )
    if rule_id.startswith(("braced_", "keyword_threshold_")) or rule_id in {
        "prefix_attack_delta",
        "prefix_health_delta",
    }:
        negative_cases.append(
            _example("ambiguous_reference_negative", positive, raw, False, refs + refs)
        )
        wrong = tuple(r.model_copy(update={"category": "card_name"}) for r in refs)
        negative_cases.append(
            _example("wrong_category_negative", positive, raw, False, wrong)
        )
    if rule_id == "bracket_choice_index":
        positive_cases.append(
            _example(
                "second_group_positive",
                positive + "\n3つチョイス\n【1】試験",
                "2",
                True,
                refs,
            )
        )
        # The first group remains independently valid; the later single label is not recognized.
        second = "3つチョイス\n【1】試験\n【2】試験\n3つチョイス\n【4】試験"
        negative_cases.append(
            _example("incomplete_later_group_negative", second, "4", False, refs)
        )
    if rule_id in {"suffix_damage_amount", "suffix_unit_items"}:
        negative_cases.append(
            _example("owned_negative", "コスト2" + SUFFIX[rule_id], "2", False, refs)
        )
