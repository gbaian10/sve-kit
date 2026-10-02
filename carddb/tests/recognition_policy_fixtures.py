"""Synthetic main histories, paired rule receipts and isolated Git author settings."""

import ast
import copy
import inspect
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest
from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical, digest, object_value
from sve_carddb.template_parameter_rules.cases import fixed_examples
from sve_carddb.template_parameter_rules.legacy import (
    ANALYSIS,
    NUMERIC,
    historical_role,
)
from sve_carddb.template_parameter_rules.loader import MATCHER_PATHS
from sve_carddb.template_parameter_rules.models import (
    LEGACY_IDS,
    PRECEDENCE,
    RESTRICTION,
)
from sve_carddb.template_parameters.numeric_rules import (
    definition as numeric_definition,
)
from sve_carddb.template_parameters.rule_candidates import BY_ID, definition
from sve_carddb.template_sources.normalizer import VERSION
from sve_carddb.template_sources.pins import PARSER

from .adoption_fixtures import commit, git

if TYPE_CHECKING:
    from sve_carddb.template_parameters.models import NumericRule

HASH = digest(b"Synthetic event evidence")
ROOT = "authored/template-parameter-rules/"
IDS = ("suffix_damage_amount", "suffix_ordinal_cards", "suffix_recovery_amount")
RUNTIME = Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class GitCase:
    repository: Path
    main: str
    historical: str


@pytest.fixture(scope="module")
def policy_git(tmp_path_factory: pytest.TempPathFactory) -> GitCase:
    root = tmp_path_factory.mktemp("recognition-git")
    for path in (*MATCHER_PATHS, "carddb/uv.lock", "carddb/pyproject.toml"):
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(RUNTIME / path, target)
    current = (root / ANALYSIS).read_text()
    parsed = ast.parse(current)
    node = next(
        n
        for n in parsed.body
        if isinstance(n, ast.FunctionDef) and n.name == "numeric_role"
    )
    source = inspect.getsource(historical_role).replace(
        "def historical_role(", "def numeric_role("
    )
    assert node.end_lineno is not None
    old = current.splitlines(keepends=True)
    old[node.lineno - 1 : node.end_lineno] = [source]
    (root / ANALYSIS).write_text("".join(old))
    numeric = (
        (root / NUMERIC)
        .read_text()
        .replace("numeric-rule-proposals-v3", "numeric-rule-proposals-v2")
    )
    (root / NUMERIC).write_text(numeric)
    git(root, "init", "-b", "main")
    historical = commit(root)
    (root / ANALYSIS).write_text(current)
    shutil.copyfile(RUNTIME / NUMERIC, root / NUMERIC)
    main = commit(root)
    return GitCase(root, main, historical)


@pytest.fixture
def policy_repository(policy_git: GitCase, tmp_path: Path) -> Path:
    root = tmp_path / "repository"
    shutil.copytree(policy_git.repository, root)
    return root


def rule(identifier: str, matcher: str) -> dict[str, JsonValue]:
    if identifier in LEGACY_IDS:
        info = numeric_definition(cast("NumericRule", identifier))
        role = "numeric"
    else:
        info, role = definition(BY_ID[identifier]), BY_ID[identifier].role
    return {
        "rule_id": identifier,
        "matcher_version": info["matcher_version"],
        "matcher_commit": matcher,
        "recognized_role": role,
        "match_conditions": info["match_conditions"],
        "condition_hash": info["condition_hash"],
        "finite_transform": "preserve_source",
        "examples": fixed_examples(identifier).model_dump(mode="json"),
    }


def identity(value: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        key: value[key]
        for key in ("rule_id", "matcher_version", "condition_hash", "matcher_commit")
    }


def event(items: list[JsonValue], *, old: bool = False) -> dict[str, JsonValue]:
    return {
        "form": "conversation_bulk",
        "reviewed_by": "Synthetic Maintainer",
        "reviewed_at": "2026-10-02T16:35:01.413Z"
        if old
        else "2026-10-02T21:03:25.150Z",
        "reviewed_precision": "instant",
        "authorization_basis": {
            "event_locator": "00000000-0000-0000-0000-000000000001"
            if old
            else "00000000-0000-0000-0000-000000000002",
            "source_locator": None,
            "evidence_hash": HASH,
            "statement": "Synthetic blanket authorization; no clicks or per-example review",
        },
        "presentation": {
            "page_hash": HASH,
            "page_payload_hash": HASH,
            "presented_rules": copy.deepcopy(items),
            "disclosures": [],
        },
        "authorized_rules": copy.deepcopy(items),
        "note": "Synthetic event; presentation evidence is not a human sample",
    }


def pair(
    matcher: str,
    *,
    bridge: bool = False,
    policy_id: str = "synthetic-v1",
    store: str = "synthetic",
    batch: str = HASH,
) -> tuple[dict[str, JsonValue], dict[str, JsonValue]]:
    ids = tuple(sorted((*LEGACY_IDS, *BY_ID))) if bridge else IDS
    rules: list[JsonValue] = [rule(identifier, matcher) for identifier in ids]
    policy: dict[str, JsonValue] = {
        "parameter_rule_policy_format": 1,
        "kind": "template_parameter_rule_policy",
        "policy_id": policy_id,
        "scope": {
            "region": "jp",
            "roles": ["body", "reminder"],
            "source_batches": [{"store_id": store, "batch_id": batch}],
            "parser_ids": [PARSER],
            "normalizer_ids": [VERSION, "template-parameters-jp-candidate-v1"],
        },
        "precedence": list[JsonValue](PRECEDENCE),
        "rules": rules,
    }
    current: list[JsonValue] = [identity(object_value(r)) for r in rules]
    new = event(current)
    events: dict[str, JsonValue] = {"event_20261002_2": new}
    if bridge:
        historical: list[JsonValue] = [
            {
                "rule_id": id_,
                "matcher_version": "numeric-rule-proposals-v2:" + id_,
                "condition_hash": None,
                "matcher_commit": None,
            }
            for id_ in LEGACY_IDS
        ]
        events["event_20261002_1"] = event(historical, old=True)
        new["authorized_rules"] = [
            i for i in current if object_value(i)["rule_id"] not in LEGACY_IDS
        ]
        object_value(new["presentation"])["disclosures"] = [
            {
                "id": RESTRICTION,
                "text": "Synthetic page notice: reminder fullwidth signs are excluded",
                "delivery": "page",
                "evidence_hash": HASH,
            }
        ]
    receipt: dict[str, JsonValue] = {
        "parameter_rule_approval_format": 1,
        "kind": "template_parameter_rule_approval",
        "policy_id": policy_id,
        "policy_hash": digest(canonical(policy)),
        "events": events,
        "rules": [
            {
                "rule_id": r["rule_id"],
                "condition_hash": r["condition_hash"],
                "matcher_commit": matcher,
                "event_id": "event_20261002_1"
                if bridge and r["rule_id"] in LEGACY_IDS
                else "event_20261002_2",
                "presented_in": "event_20261002_2",
                "restriction_ids": [RESTRICTION]
                if bridge and r["rule_id"] in LEGACY_IDS
                else [],
                "note": "Synthetic page-only notices; no oral reiteration or human samples",
            }
            for value in rules
            for r in (object_value(value),)
        ],
        "note": "Synthetic fixture only; no actual receipt or human review",
    }
    return policy, receipt


def publish(
    repository: Path, policy: dict[str, JsonValue], receipt: dict[str, JsonValue]
) -> str:
    identifier = str(policy["policy_id"])
    (repository / ROOT).mkdir(parents=True, exist_ok=True)
    receipt["policy_hash"] = digest(canonical(policy))
    (repository / (ROOT + identifier + ".policy.yaml")).write_bytes(canonical(policy))
    (repository / (ROOT + identifier + ".approval.yaml")).write_bytes(
        canonical(receipt)
    )
    return commit(repository)
