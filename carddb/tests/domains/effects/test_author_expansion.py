"""Candidate goldens do not imply source admission or execution qualification."""

import hashlib
from copy import deepcopy
from typing import TYPE_CHECKING
from unittest.mock import Mock

import pytest

from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    BodyLibrary,
    ExpansionError,
    ability_nodes,
    body_payload,
    body_version,
    equivalent_ability,
)

from ...contracts.test_author_abilities import HEADS
from .test_author_entries import GOLDEN, SCHEMAS, ability, companion, document, shard

if TYPE_CHECKING:
    from pydantic import JsonValue


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


def registered(head: dict[str, JsonValue]) -> dict[str, JsonValue]:
    record = companion(head)
    record["params"] = {}
    record["registrations"] = [
        {"id": "all", "domain": {"regions": ["jp"]}, "reviews": []}
    ]
    record["body_version"] = body_version(record)
    return record


def generated(
    record: dict[str, JsonValue], params: JsonValue | None = None
) -> dict[str, JsonValue]:
    result = ability({})
    del result["ports"]
    result["generated_by"] = {
        "macro": {
            "frame_id": record["frame_id"],
            "ability_scope": record["ability_scope"],
            "applicability_scope": "all",
        },
        "version": record["body_version"],
        "params": {} if params is None else params,
    }
    return result


@pytest.mark.parametrize(("key", "payload"), HEADS.items())
def test_seventeen_heads_exact_equivalence(
    expander: AuthorExpander, key: str, payload: JsonValue
) -> None:
    record = registered({key: payload})
    result = expander.expand(
        canonical(document(generated(record))), [canonical(shard(record))]
    )
    actual = next(ability_nodes(result.tree))[1]
    assert equivalent_ability(actual) == equivalent_ability(ability({key: payload}))
    assert result.sidecar[0].source_proof == "none"
    assert result.sidecar[0].body_payload == body_payload(record)


@pytest.mark.parametrize(
    "field", ["frame_id", "ability_scope", "version", "applicability_scope"]
)
def test_exact_selection_no_fallback(expander: AuthorExpander, field: str) -> None:
    record = registered({"spell": {"do": []}})
    use = generated(record)
    generator = object_value(use["generated_by"])
    if field == "version":
        generator[field] = "effect-body-v1:sha256:" + "f" * 64
    else:
        object_value(generator["macro"])[field] = (
            "frame:" + "f" * 64 if field == "frame_id" else "missing"
        )
    with pytest.raises(ExpansionError):
        expander.expand(canonical(document(use)), [canonical(shard(record))])


def parameter_record(
    head: dict[str, JsonValue], kind: str = "Nat", component: str = "value"
) -> dict[str, JsonValue]:
    record = registered(head)
    record["params"] = {
        "N": {"type": kind, "source": {"slot": "count", "component": component}}
    }
    record["body_version"] = body_version(record)
    return record


def test_leaf_only_replacement_and_declaration_negatives(
    expander: AuthorExpander,
) -> None:
    record = parameter_record({"spell": {"do": [{"draw": {"n": {"param": "N"}}}]}})
    result = expander.expand(
        canonical(document(generated(record, {"N": {"type": "Nat", "value": 2}}))),
        [canonical(shard(record))],
    )
    assert next(ability_nodes(result.tree))[1]["spell"] == {"do": [{"draw": {"n": 2}}]}
    invalid_params: list[JsonValue] = [
        {},
        {"extra": {"type": "Nat", "value": 2}},
        {"N": {"type": "Int", "value": 2}},
    ]
    for params in invalid_params:
        with pytest.raises(ExpansionError):
            expander.expand(
                canonical(document(generated(record, params))),
                [canonical(shard(record))],
            )
    for component in ("constant", "negated_magnitude"):
        bad = parameter_record(object_value(record["body"]), component=component)
        with pytest.raises(ExpansionError, match="param_component_mismatch"):
            expander.expand(
                canonical(document(generated(bad, {"N": {"type": "Nat", "value": 2}}))),
                [canonical(shard(bad))],
            )
    unused = parameter_record({"spell": {"do": []}})
    with pytest.raises(ExpansionError, match="unused_param_declaration"):
        expander.expand(
            canonical(document(generated(unused, {"N": {"type": "Nat", "value": 2}}))),
            [canonical(shard(unused))],
        )


BP_BYTES = b'{"fanfare":{"define":{"X":{"minus":[{"count":{"who":"self","zone":"deck"}},5]}},"do":[{"draw":{"n":"X"}}]},"ports":{"exports":{},"imports":{}}}'
CP_BYTES = '{"evolve_time":{"do":[{"search":{"n":1,"result":"searched","sel":{"not_same_name_as_this":true,"trait":"ドラゴンズネスト","who":"self","zone":"deck"},"to":"ex"}},{"apply":{"effect":{"cost_mod":{"delta":-2,"when_playing":{"in":"affected"}}},"on":{"follow":"searched"},"until":"turn"}}],"ub":true},"ports":{"exports":{},"imports":{}}}'.encode()


@pytest.mark.parametrize(
    ("card", "expected", "leaf_path", "kind", "component", "value"),
    [
        (
            "BP08-038",
            BP_BYTES,
            ("fanfare", "define", "X", "minus", 1),
            "Int",
            "value",
            5,
        ),
        (
            "CP04-060",
            CP_BYTES,
            ("evolve_time", "do", 1, "apply", "effect", "cost_mod", "delta"),
            "Int",
            "negated_magnitude",
            -2,
        ),
    ],
    ids=["bp08", "cp04"],
)
def test_real_card_kernel_goldens(  # ruff: ignore[too-many-arguments, too-many-positional-arguments] -- Matrix axes remain visible to each fixed golden.
    expander: AuthorExpander,
    card: str,
    expected: bytes,
    leaf_path: tuple[str | int, ...],
    kind: str,
    component: str,
    value: int,
) -> None:
    tree = expander.schemas.author_tree(GOLDEN.read_bytes())
    entry = object_value(
        array(
            object_value(object_value(object_value(tree)["cards"])[card])["abilities"]
        )[0]
    )
    assert equivalent_ability(entry) == expected
    head = {
        key: deepcopy(child)
        for key, child in entry.items()
        if key not in {"line", "section", "sources", "ports", "imports"}
    }
    node: JsonValue = head
    for part in leaf_path[:-1]:
        node = object_value(node)[part] if isinstance(part, str) else array(node)[part]
    last = leaf_path[-1]
    if isinstance(last, str):
        object_value(node)[last] = {"param": "N"}
    else:
        array(node)[last] = {"param": "N"}
    record = parameter_record(head, kind, component)
    macro = generated(record, {"N": {"type": kind, "value": value}})
    macro["line"], macro["sources"] = entry["line"], entry["sources"]
    result = expander.expand(canonical(document(macro)), [canonical(shard(record))])
    actual = next(ability_nodes(result.tree))[1]
    assert equivalent_ability(actual) == expected
    assert result.sidecar[0].source_proof == "none"
    altered = deepcopy(actual)
    object_value(altered["ports"])["exports"] = {"out": {"path": [], "bind": "x"}}
    assert equivalent_ability(altered) != expected
    if card == "BP08-038":
        expression = object_value(
            object_value(object_value(altered["fanfare"])["define"])["X"]
        )
        expression["plus"] = expression.pop("minus")
    else:
        steps = array(object_value(altered["evolve_time"])["do"])
        object_value(object_value(object_value(steps[1])["apply"])["on"])["follow"] = (
            "other"
        )
        steps.reverse()
    assert equivalent_ability(altered) != expected


def test_hash_collision_compares_complete_payload(
    expander: AuthorExpander, monkeypatch: pytest.MonkeyPatch
) -> None:
    first = registered({"spell": {"do": []}})
    second = registered({"quick": True})
    fixed = "a" * 64
    first["body_version"] = second["body_version"] = "effect-body-v1:sha256:" + fixed
    monkeypatch.setattr(
        hashlib, "sha256", Mock(return_value=Mock(hexdigest=Mock(return_value=fixed)))
    )
    with pytest.raises(ExpansionError, match="body_hash_collision"):
        BodyLibrary(
            expander.schemas, [canonical(shard(first)), canonical(shard(second))]
        )
