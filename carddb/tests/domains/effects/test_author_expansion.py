"""Candidate goldens do not imply source admission or execution qualification."""

import hashlib
from copy import deepcopy
from typing import TYPE_CHECKING
from unittest.mock import Mock

import pytest

from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    BodyLibrary,
    ExpansionError,
    ability_nodes,
    body_payload,
    body_version,
    equivalent_ability,
)
from sve_carddb.domains.effects.author_sites import SITE_RULES, NodeSites, SiteError

from .test_author_entries import (
    GOLDEN,
    HEADS,
    NESTED,
    SCHEMAS,
    ability,
    companion,
    document,
    shard,
)

if TYPE_CHECKING:
    from pathlib import Path

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


def test_p1_hash_and_omission(expander: AuthorExpander) -> None:
    record = registered({"spell": {"do": []}})
    assert (
        body_version(record)
        == "effect-body-v1:sha256:d4b82bd23c514f4ec3167e143885706c8428456bce2e037c1a4bbe6fb218c3d7"
    )
    original = body_payload(record)
    record["note"] = "review annotation"
    assert body_payload(record) == original
    object_value(object_value(record["body"])["spell"])["define"] = {"X": 1}
    assert body_payload(record) != original
    record["body_version"] = body_version(record)
    library = BodyLibrary(expander.schemas, [canonical(shard(record))])
    assert library.select(
        str(record["frame_id"]), "ability_0", str(record["body_version"])
    ).payload == body_payload(record)


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


def test_duplicate_mismatch_and_domain(expander: AuthorExpander) -> None:
    record = registered({"spell": {"do": []}})
    raw = canonical(shard(record))
    with pytest.raises(ExpansionError, match="duplicate_body"):
        BodyLibrary(expander.schemas, [raw, raw])
    bad = deepcopy(record)
    bad["body_version"] = "effect-body-v1:sha256:" + "f" * 64
    with pytest.raises(ExpansionError, match="body_version_mismatch"):
        BodyLibrary(expander.schemas, [canonical(shard(bad))])
    object_value(array(bad["registrations"])[0])["domain"] = {"regions": ["en"]}
    bad["body_version"] = body_version(bad)
    with pytest.raises(ExpansionError, match="domain_excludes_jp"):
        expander.expand(canonical(document(generated(bad))), [canonical(shard(bad))])


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


@pytest.mark.parametrize(("head", "expected_path"), NESTED)
def test_nested_sites_preserve_exact_owner(
    expander: AuthorExpander,
    head: dict[str, JsonValue],
    expected_path: tuple[str | int, ...],
) -> None:
    found = expander.sites.parameters(head)
    assert {site.path for site in found} == {expected_path}
    if "replace_with" in expected_path or "after" in expected_path:
        assert found[0].owner == "A.act.draw"


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


def test_only_top_level_metadata_is_removed() -> None:
    assert (
        equivalent_ability(
            {"line": 9, "ports": {}, "head": {"sources": [1], "line": 3}}
        )
        == b'{"head":{"line":3,"sources":[1]},"ports":{}}'
    )


def test_declared_sites_cover_schema_graph(  # ruff: ignore[complex-structure] -- Independently enumerate every schema path, including forbidden conditions.
    expander: AuthorExpander,
) -> None:
    definitions = expander.sites.definitions
    found: set[tuple[str, tuple[str | int, ...]]] = set()

    def visit(  # ruff: ignore[complex-structure] -- Static traversal is independent of the instance visitor under test.
        schema: JsonValue, owner: str, path: tuple[str | int, ...], seen: frozenset[str]
    ) -> None:
        if not isinstance(schema, dict):
            return
        reference = schema.get("$ref", schema.get("$dynamicRef"))
        if isinstance(reference, str):
            name = (
                "A.param"
                if reference.endswith("#authorParam")
                else reference.split("/$defs/")[-1]
            )
            if name == "A.param":
                found.add((owner, path))
                return
            target = definitions.get(name)
            if (
                isinstance(target, dict)
                and "properties" not in target
                and name not in seen
            ):
                visit(target, owner, path, seen | {name})
        for key in ("anyOf", "oneOf", "allOf"):
            for child in array(schema.get(key, [])):
                visit(child, owner, path, seen)
        for index, child in enumerate(array(schema.get("prefixItems", []))):
            visit(child, owner, (*path, index), seen)
        if "items" in schema:
            visit(schema["items"], owner, (*path, "*"), seen)
        for forbidden in ("not", "contains", "if"):
            before = set(found)
            if forbidden in schema:
                visit(schema[forbidden], owner, path, seen)
            assert found == before

    for name, schema in definitions.items():
        if isinstance(schema, dict) and name != "A.param":
            for key, child in object_value(schema.get("properties", {})).items():
                visit(child, name, (key,), frozenset({name}))
    assert len(found) == 55
    assert found == {(rule.owner, rule.path) for rule in SITE_RULES}


def test_keyword_mutation_fails_closed(tmp_path: Path) -> None:
    schema = object_value(parse((SCHEMAS / "author-nodes.schema.json").read_bytes()))
    object_value(object_value(schema["$defs"])["A.act.draw"])["futureTraversal"] = {}
    (tmp_path / "author-nodes.schema.json").write_bytes(canonical(schema))
    with pytest.raises(SiteError, match="unsupported_schema_keyword"):
        NodeSites(tmp_path).walk({"spell": {"do": [{"draw": {"n": 1}}]}})


@pytest.mark.parametrize(
    ("kind", "head", "value"),
    [
        (
            "Int",
            {"spell": {"define": {"X": {"minus": [9, {"param": "N"}]}}, "do": []}},
            3,
        ),
        ("Nat", {"spell": {"do": [{"draw": {"n": {"param": "N"}}}]}}, 2),
        (
            "CardName",
            {
                "spell": {
                    "do": [{"create": {"token": {"param": "N"}, "n": 1, "to": "field"}}]
                }
            },
            "ナイト",
        ),
        ("Concept", {"keywords": [{"param": "N"}]}, "ward"),
        (
            "CardKind",
            {
                "spell": {
                    "do": [
                        {
                            "search": {
                                "sel": {
                                    "who": "self",
                                    "zone": "deck",
                                    "kind": {"param": "N"},
                                },
                                "n": 1,
                                "to": "hand",
                            }
                        }
                    ]
                }
            },
            "follower",
        ),
        (
            "Player",
            {"spell": {"do": [{"draw": {"n": 1, "who": {"param": "N"}}}]}},
            "opp",
        ),
        (
            "Zone",
            {
                "spell": {
                    "do": [
                        {"create": {"token": "ナイト", "n": 1, "to": {"param": "N"}}}
                    ]
                }
            },
            "field",
        ),
        (
            "Phase",
            {
                "auto": {
                    "on": {"phase": {"which": {"param": "N"}, "whose": "self"}},
                    "do": [],
                }
            },
            "end",
        ),
    ],
)
def test_eight_parameter_types(
    expander: AuthorExpander, kind: str, head: dict[str, JsonValue], value: JsonValue
) -> None:
    record = parameter_record(head, kind)
    result = expander.expand(
        canonical(document(generated(record, {"N": {"type": kind, "value": value}}))),
        [canonical(shard(record))],
    )
    assert result.sidecar[0].source_proof == "none"
    expander.schemas.expanded(canonical(result.tree))


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


def test_slot_change_invalidates_exact_body_version(expander: AuthorExpander) -> None:
    record = parameter_record({"spell": {"do": [{"draw": {"n": {"param": "N"}}}]}})
    object_value(object_value(object_value(record["params"])["N"])["source"])[
        "slot"
    ] = "different"
    with pytest.raises(ExpansionError, match="body_version_mismatch"):
        BodyLibrary(expander.schemas, [canonical(shard(record))])
