"""A finite v2 adapter proves the sole supported v3 change without executing Git code."""

import ast
import re
from pathlib import Path
from typing import TYPE_CHECKING, override

from sve_carddb.template_parameter_rules.repository import ancestor, git
from sve_carddb.template_parameters.analysis import Position, numeric_role
from sve_carddb.template_parameters.numeric_rules import (
    NUMERIC_PREFIX,
    NUMERIC_RULE_PENDING,
    NUMERIC_RULES,
    NUMERIC_SUFFIX,
    excluded,
)

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_parameters.models import NumericRule

ANALYSIS = "carddb/src/sve_carddb/template_parameters/analysis.py"
NUMERIC = "carddb/src/sve_carddb/template_parameters/numeric_rules.py"
RUNTIME = Path(__file__).resolve().parents[4]


def historical_role(
    normalized: str, position: Position
) -> tuple[NumericRule | None, tuple[str, ...]]:
    """Replay the original v2 kernel; only the reminder fullwidth signs differ."""
    before = normalized[: position.start]
    after = normalized[position.end :]
    if before.endswith(("-", "+", "−")):
        return None, ("signed_numeric_requires_review",)
    if (
        (before and re.search(r"[A-Za-z0-9_]$", before))
        or re.match(r"^[A-Za-z0-9_]", after)
    ) and not (NUMERIC_PREFIX.search(before) or after.startswith("PP")):
        return None, ("numeric_identifier_requires_review",)
    if (reason := excluded(after)) is not None:
        return None, (reason,)
    match = NUMERIC_SUFFIX.match(after) or NUMERIC_PREFIX.search(before)
    if match is not None:
        rule = next(rule for rule in NUMERIC_RULES if rule == match.lastgroup)
        return rule, (NUMERIC_RULE_PENDING,)
    return None, ("numeric_role_requires_review",)


def _functions(raw: bytes) -> dict[str, ast.FunctionDef]:
    result = {}
    for node in ast.parse(raw).body:
        if isinstance(node, ast.FunctionDef):
            if (
                node.body
                and isinstance(node.body[0], ast.Expr)
                and isinstance(node.body[0].value, ast.Constant)
                and isinstance(node.body[0].value.value, str)
            ):
                node.body.pop(0)
            result[node.name] = node
    return result


def _dump(node: ast.FunctionDef, name: str) -> str:
    node.name = name
    return ast.dump(node, include_attributes=False)


def _constants(raw: bytes) -> dict[str, object]:
    result: dict[str, object] = {}
    for node in ast.parse(raw).body:
        if (
            isinstance(node, ast.Assign)
            and len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name)
        ):
            name, value = node.targets[0].id, node.value
        elif (
            isinstance(node, ast.AnnAssign)
            and isinstance(node.target, ast.Name)
            and node.value is not None
        ):
            name, value = node.target.id, node.value
        else:
            continue
        if (
            isinstance(value, ast.Call)
            and isinstance(value.func, ast.Attribute)
            and value.func.attr == "compile"
            and len(value.args) == 1
        ):
            value = value.args[0]
        try:
            result[name] = ast.literal_eval(value)
        except ValueError, TypeError:
            continue
    return result


class CurrentKernel(ast.NodeTransformer):
    @staticmethod
    @override
    def visit_Name(node: ast.Name) -> ast.AST:
        """Unpack only the documented constant relocation and the two added signs."""
        constants = {
            "SIGNS": ("-", "+", "−"),
            "ASCII_BEFORE": r"[A-Za-z0-9_]$",
            "ASCII_AFTER": r"^[A-Za-z0-9_]",
            "RESOURCE_PREFIX_EXCEPTION": "PP",
        }
        if node.id not in constants:
            return node
        return ast.parse(repr(constants[node.id]), mode="eval").body


def validate_v2(analysis: bytes, numeric: bytes) -> None:
    """Check the complete analysis functions plus every matching grammar dependency."""
    actual = _functions(analysis)
    current = _functions((RUNTIME / ANALYSIS).read_bytes())
    original_classes = [
        ast.dump(n) for n in ast.parse(analysis).body if isinstance(n, ast.ClassDef)
    ]
    current_classes = [
        ast.dump(n)
        for n in ast.parse((RUNTIME / ANALYSIS).read_bytes()).body
        if isinstance(n, ast.ClassDef)
    ]
    if original_classes != current_classes:
        raise ValueError(
            "Recognition historical position model differs from the supported adapter"
        )
    expected = _functions(Path(__file__).read_bytes())["historical_role"]
    if "numeric_role" not in actual or _dump(actual["numeric_role"], "kernel") != _dump(
        expected, "kernel"
    ):
        raise ValueError("Recognition historical numeric kernel is unsupported")
    kernel = CurrentKernel().visit(current["numeric_role"])
    assert isinstance(kernel, ast.FunctionDef)
    if _dump(kernel, "kernel") != _dump(expected, "kernel"):
        raise ValueError(
            "Recognition current numeric kernel exceeds the finite historical adapter"
        )
    for name, function in current.items():
        if name == "numeric_role":
            continue
        if name not in actual or ast.dump(actual[name]) != ast.dump(function):
            raise ValueError(
                "Recognition historical source provenance differs from the supported adapter"
            )
    constants = _constants(analysis) | _constants(numeric)
    current_constants = _constants((RUNTIME / ANALYSIS).read_bytes()) | _constants(
        (RUNTIME / NUMERIC).read_bytes()
    )
    if current_constants.get("SIGNS") != ("-", "+", "−", "＋", "－"):
        raise ValueError(
            "Recognition current sign guard exceeds the finite historical restriction"
        )
    required = (
        "SAFE_INTEGER",
        "BRACED",
        "NUMERIC_SUFFIX",
        "NUMERIC_PREFIX",
        "NUMERIC_RULES",
        "NUMERIC_RULE_PENDING",
        "RECOVERY",
        "ORDINAL",
        "RECOVERY_PENDING",
        "ORDINAL_PENDING",
    )
    if constants.get("VERSION") != "numeric-rule-proposals-v2" or any(
        constants.get(k) != current_constants[k] for k in required
    ):
        raise ValueError("Recognition historical numeric grammar is unsupported")
    old_excluded = _functions(numeric).get("excluded")
    if old_excluded is None or ast.dump(old_excluded) != ast.dump(
        _functions((RUNTIME / NUMERIC).read_bytes())["excluded"]
    ):
        raise ValueError(
            "Recognition historical lexical veto differs from the supported adapter"
        )


def historical(repository: PinnedRepository, main: str) -> tuple[str, dict[str, bytes]]:
    """Find the registered original v2 source in pinned main history, never a branch."""
    versions = (
        git(repository, "rev-list", main, "--", ANALYSIS, NUMERIC).decode().splitlines()
    )
    for version in versions:
        try:
            content = repository.read_many(version, (ANALYSIS, NUMERIC))
        except ValueError:
            continue
        if _constants(content[NUMERIC]).get("VERSION") != "numeric-rule-proposals-v2":
            continue
        validate_v2(content[ANALYSIS], content[NUMERIC])
        ancestor(repository, version, main)
        return version, content
    raise ValueError(
        "Recognition original v2 matcher is absent from pinned main history"
    )


def assert_restriction(normalized: str, position: Position, *, reminder: bool) -> None:
    """No additional exclusion or changed role can borrow the original eight approvals."""
    old = historical_role(normalized, position)
    current = numeric_role(normalized, position)
    if old == current:
        return
    before = normalized[: position.start]
    if not (
        reminder
        and before.endswith(("＋", "－"))
        and current == (None, ("signed_numeric_requires_review",))
    ):
        raise ValueError(
            "Recognition historical bridge changes more than the sign restriction"
        )
