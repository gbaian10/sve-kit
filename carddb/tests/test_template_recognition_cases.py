"""Independent case evidence and finite historical grammar checks."""

import ast
import re

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.template_parameter_rules import legacy
from sve_carddb.template_parameter_rules.cases import _example, evaluate, fixed_examples
from sve_carddb.template_parameter_rules.legacy import (
    ANALYSIS,
    NUMERIC,
    historical,
    validate_v2,
)
from sve_carddb.template_parameter_rules.models import LEGACY_IDS, RoleRange
from sve_carddb.template_parameters.rule_candidates import BY_ID

from .recognition_policy_fixtures import GitCase, policy_git

__all__ = ("policy_git",)


def test_current_signs_must_be_exactly_the_finite_restriction(
    policy_git: GitCase, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = legacy._constants

    def constants(raw: bytes) -> dict[str, object]:
        result = original(raw)
        if result.get("VERSION") == "numeric-rule-proposals-v3":
            result["SIGNS"] = ("-", "+", "−")
        return result

    content = PinnedRepository(policy_git.repository).read_many(
        policy_git.historical, (ANALYSIS, NUMERIC)
    )
    monkeypatch.setattr(legacy, "_constants", constants)
    with pytest.raises(
        ValueError,
        match=r"^Recognition current sign guard exceeds the finite historical restriction$",
    ):
        validate_v2(content[ANALYSIS], content[NUMERIC])


@pytest.mark.parametrize("rule_id", sorted((*LEGACY_IDS, *BY_ID)))
def test_fixed_positive_and_negative_sets_replay(rule_id: str) -> None:
    cases = fixed_examples(rule_id)
    for case in cases.positive:
        assert evaluate(rule_id, case.input) is True
    for case in cases.negative:
        assert evaluate(rule_id, case.input) is False


@pytest.mark.parametrize(
    ("text", "raw", "rule", "expected"),
    [
        ("コスト2ダメージ", "2", "suffix_damage_amount", False),
        ("コスト2回復", "2", "suffix_recovery_amount", True),
        ("コスト2枚目", "2", "suffix_ordinal_cards", True),
        ("試験2回復A", "2", "suffix_recovery_amount", False),
        ("試験2枚目A", "2", "suffix_ordinal_cards", False),
        ("（コストを＋2試験）", "2", "prefix_cost_delta", False),
        ("（コストを－2試験）", "2", "prefix_cost_delta", False),
        ("試験0枚目", "0", "suffix_ordinal_cards", False),
        ("試験②枚", "②", "suffix_unit_cards", False),
        ("（3つチョイス）\n【1】試験\n【2】試験", "1", "bracket_choice_index", False),
        ("【1】試験\n【2】試験\n3つチョイス", "1", "bracket_choice_index", False),
        ("3つチョイス\n【2】試験\n【1】試験", "1", "bracket_choice_index", False),
        ("3つチョイス\n【1】試験\n【1】試験", "1", "bracket_choice_index", False),
        ("3つチョイス\n【1】試験\n【3】試験", "1", "bracket_choice_index", False),
        ("【1】試験\n【2】試験", "1", "bracket_choice_index", False),
    ],
)
def test_independent_minimal_boundaries(
    text: str, raw: str, rule: str, expected: bool
) -> None:
    example = _example("independent", text, raw, expected, ())
    assert evaluate(rule, example.input) is expected


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("role", "Recognition case partition differs from recomputed roles"),
        (
            "missing_invalid",
            "Recognition case causes or old ownership differ from replay",
        ),
        (
            "invented_cause",
            "Recognition case causes or old ownership differ from replay",
        ),
        ("owned", "Recognition case causes or old ownership differ from replay"),
    ],
)
def test_case_declarations_are_never_trusted(mutation: str, message: str) -> None:
    text = (
        "（試験2回復）"
        if mutation == "role"
        else "試験②枚"
        if mutation == "missing_invalid"
        else "コスト2ダメージ"
    )
    raw = "②" if mutation == "missing_invalid" else "2"
    case = _example("tamper", text, raw, False, ()).input
    if mutation == "role":
        case = case.model_copy(
            update={
                "role": "body",
                "role_spans": (RoleRange(role="body", start=0, end=len(text)),),
            }
        )
    elif mutation == "missing_invalid":
        case = case.model_copy(
            update={
                "original_issues": tuple(
                    i
                    for i in case.original_issues
                    if i != "invalid_safe_unsigned_decimal"
                )
            }
        )
    elif mutation == "owned":
        case = case.model_copy(update={"existing_numeric_rule": None})
    else:
        case = case.model_copy(
            update={
                "original_issues": tuple(
                    sorted((*case.original_issues, "numeric_role_requires_review"))
                )
            }
        )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        evaluate("suffix_damage_amount", case)


def test_historical_v2_adapter_is_main_reachable(policy_git: GitCase) -> None:
    revision, files = historical(
        PinnedRepository(policy_git.repository), policy_git.main
    )
    assert revision == policy_git.historical
    validate_v2(files[ANALYSIS], files[NUMERIC])


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("kernel", "Recognition historical numeric kernel is unsupported"),
        (
            "provenance",
            "Recognition historical source provenance differs from the supported adapter",
        ),
        ("constant", "Recognition historical numeric grammar is unsupported"),
        (
            "veto",
            "Recognition historical lexical veto differs from the supported adapter",
        ),
    ],
)
def test_historical_adapter_is_closed(
    policy_git: GitCase, mutation: str, message: str
) -> None:
    repo = PinnedRepository(policy_git.repository)
    analysis, numeric = (
        repo.read(policy_git.historical, ANALYSIS),
        repo.read(policy_git.historical, NUMERIC),
    )
    if mutation == "kernel":
        analysis = analysis.replace(b'after.startswith("PP")', b'after.startswith("P")')
    elif mutation == "provenance":
        tree = ast.parse(analysis)
        tree.body = [
            n
            for n in tree.body
            if not isinstance(n, ast.FunctionDef) or n.name != "literals"
        ]
        analysis = ast.unparse(tree).encode()
    elif mutation == "constant":
        numeric = (
            numeric.replace(b"SAFE_INTEGER = 9007199254740991", b"SAFE_INTEGER = 1")
            if b"SAFE_INTEGER" in numeric
            else numeric.replace(
                b"(?P<suffix_unit_cards>", b"(?P<suffix_unit_cards_changed>"
            )
        )
    else:
        numeric = numeric.replace(b"if RECOVERY.match(after):", b"if False:")
    assert analysis != repo.read(
        policy_git.historical, ANALYSIS
    ) or numeric != repo.read(policy_git.historical, NUMERIC)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        validate_v2(analysis, numeric)
