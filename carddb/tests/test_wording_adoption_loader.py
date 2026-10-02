"""Inventory and immutable byte checks are independent of current selection."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical
from sve_carddb.wording_adoptions.loader import load_adoptions

from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic registry
from .text_observation_fixtures import make_case
from .wording_adoption_fixtures import commit as commit_wording_inputs
from .wording_adoption_fixtures import git

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.review import Inputs


def commit(root: Path) -> str:
    git(root, "init")
    return commit_wording_inputs(root)


@pytest.mark.parametrize(
    "change", ["none", "missing", "bytes", "unindexed", "path", "format", "symlink"]
)
def test_empty_adoption_inventory_is_explicit_and_immutable(
    tmp_path: Path, inputs: Inputs, change: str
) -> None:
    case = make_case(tmp_path / "authored", inputs, regions=("jp",))
    directory = case.root / "wording-adoptions"
    directory.mkdir()
    index = directory / "index.yaml"
    wire: dict[str, JsonValue] = {
        "wording_adoption_format": 1,
        "kind": "wording_adoption_index",
        "includes": {},
    }
    index.write_bytes(canonical(wire))
    revision = commit(tmp_path)
    if change == "missing":
        index.unlink()
    elif change == "bytes":
        index.write_bytes(index.read_bytes() + b"# different exact bytes\n")
    elif change == "unindexed":
        (directory / "jp").mkdir()
        (directory / "jp/001.yaml").write_bytes(b"{}\n")
    elif change == "path":
        index.write_bytes(
            canonical(wire | {"includes": {"../outside.yaml": "sha256:" + "1" * 64}})
        )
    elif change == "format":
        index.write_bytes(canonical(wire | {"wording_adoption_format": True}))
    elif change == "symlink":
        target = tmp_path / "index-copy.yaml"
        target.write_bytes(index.read_bytes())
        index.unlink()
        index.symlink_to(target)
    if change != "none":
        errors = {
            "missing": "Missing adoption input file",
            "bytes": "bytes differ from the pinned authored revision",
            "unindexed": "indexed file closure differs from disk",
            "path": "Unsafe adoption shard path",
            "format": "Invalid wording adoption fields",
            "symlink": "input symlinks are forbidden",
        }
        with pytest.raises(ValueError, match=errors[change]):
            load_adoptions(
                case.root,
                authored_revision=revision,
                registry=case.identity.snapshot,
                stores={},
            )
    else:
        result = load_adoptions(
            case.root,
            authored_revision=revision,
            registry=case.identity.snapshot,
            stores={},
        )
        assert not result.records
        assert not result.shards
        assert not result.decisions
        assert not result.closure
