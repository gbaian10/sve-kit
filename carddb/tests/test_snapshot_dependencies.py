"""Dependency validation must precede graphlib's implicit predecessor insertion."""

import pytest
from pydantic import JsonValue

from sve_carddb.core.json import array, object_value
from sve_carddb.snapshot.reader import _files


def manifest(graph: dict[str, list[str]]) -> dict[str, JsonValue]:
    """Build only the file-boundary fields, without unrelated payload fixtures."""
    return {
        "files": [
            {
                "key": key,
                "sha256": "sha256:" + "0" * 64,
                "dependencies": [
                    {"key": dep, "sha256": "sha256:" + "0" * 64} for dep in dependencies
                ],
            }
            for key, dependencies in sorted(graph.items())
        ]
    }


@pytest.mark.parametrize(
    "graph",
    [
        {},
        {"a": []},
        {"a": ["b"], "b": ["c"], "c": []},
        {"a": [], "b": ["a"], "c": ["a", "b"]},
        {"a": ["b", "c"], "b": ["d"], "c": ["d"], "d": [], "e": []},
    ],
    ids=["empty", "isolated", "reverse-chain", "forward-chain", "diamond"],
)
def test_acyclic_dependencies_keep_manifest_order(
    graph: dict[str, list[str]],
) -> None:
    files = _files(manifest(graph))
    assert list(files) == sorted(graph)
    assert len(files) == len(graph)


@pytest.mark.parametrize(
    "graph",
    [
        {"a": ["a"]},
        {"a": ["b"], "b": ["c"], "c": ["a"]},
        {"a": [], "b": ["a"], "c": ["d"], "d": ["c"], "e": []},
    ],
    ids=["self", "three-nodes", "cycle-after-ready-nodes"],
)
def test_cycles_fail_with_the_existing_public_error(
    graph: dict[str, list[str]],
) -> None:
    with pytest.raises(ValueError, match=r"^Cyclic dependencies$"):
        _files(manifest(graph))


def test_missing_predecessor_is_not_inserted_by_graphlib() -> None:
    with pytest.raises(KeyError, match=r"^'missing'$"):
        _files(manifest({"a": ["missing"]}))


def test_reference_hash_cannot_borrow_an_existing_predecessor() -> None:
    data = manifest({"a": [], "b": ["a"]})
    reference = object_value(
        array(object_value(array(data["files"])[1])["dependencies"])[0]
    )
    reference["sha256"] = "sha256:" + "1" * 64
    with pytest.raises(ValueError, match=r"^Dependency hash mismatch$"):
        _files(data)


@pytest.mark.parametrize("dependencies", [["a", "a"], ["b", "a"]])
def test_dependency_list_remains_sorted_and_unique(
    dependencies: list[str],
) -> None:
    with pytest.raises(ValueError, match=r"^Expected sorted unique values$"):
        _files(manifest({"a": [], "b": [], "c": dependencies}))
