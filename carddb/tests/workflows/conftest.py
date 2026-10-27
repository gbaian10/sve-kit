"""Share real authoring inputs; writers receive private copies of a small inventory."""

import shutil
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, object_value, parse, string
from sve_carddb.core.yaml import JSON_VALUE, parse_yaml
from sve_carddb.workflows import authoring

from ..support.authoring_cli_fixtures import RepositoryCheck, repository_check
from ..support.time_budget import slow_fixture

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator

    from pydantic import JsonValue

REPO = Path(__file__).resolve().parents[3]


def read_tree(path: Path) -> dict[str, JsonValue]:
    return object_value(JSON_VALUE.validate_python(parse_yaml(path.read_bytes())))


@pytest.fixture(scope="session")
def authoring_inputs() -> authoring.Inputs:
    batch = string(
        object_value(parse((REPO / "tests/engine/card-input.json").read_bytes()))[
            "batch_id"
        ]
    )
    return authoring.Inputs(
        profile="public",
        repo=REPO,
        jp_batch_id=batch,
        revision="1036aa89bb1ce52cf4a964df8461663ab644b614",
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the real public profile, source database and translation registry once per worker for authoring integration tests"
)
def repository_workspace(
    authoring_inputs: authoring.Inputs,
) -> Iterator[authoring.Workspace]:
    with authoring.prepare(
        authoring_inputs,
        cards=frozenset(
            {"BP09-005", "BP01-138", "BP19-P32", "BP01-048", "BP01-046", "BP01-176"}
        ),
    ) as prepared:
        # Tests write authored copies; the shared source database stays read-only.
        prepared.current.db._connection.execute("PRAGMA query_only=ON")
        yield prepared


@pytest.fixture(scope="session")
def authoring_inventory(tmp_path_factory: pytest.TempPathFactory) -> Path:
    root = tmp_path_factory.mktemp("authoring-inventory") / "authored"
    index = read_tree(REPO / "authored/rules/effects-index.yaml")
    numbers = {"BP01-110", "BP06-048", "BP10-122", "ECP01-002"}
    records = [
        item
        for item in array(index["records"])
        if object_value(item)["record_key"] in numbers
    ]
    index["records"] = records
    bodies: set[str] = set()
    for name in sorted({string(object_value(item)["file"]) for item in records}):
        tree = read_tree(REPO / "authored" / name)
        cards = object_value(tree["cards"])
        tree["cards"] = {
            number: card for number, card in cards.items() if number in numbers
        }
        for card in object_value(tree["cards"]).values():
            for ability in array(object_value(card)["abilities"]):
                use = object_value(ability).get("use")
                if isinstance(use, str):
                    bodies.add(use)
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(authoring.format_yaml(tree))
    for owner in ("BP01", "BP06", "BP10", "ECP01"):
        if owner != "BP01":
            source = REPO / f"authored/rules/effect-bodies/{owner}/000.yaml"
            target = root / source.relative_to(REPO / "authored")
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
    # A BP01 sibling keeps deletion past the nonempty-card schema check.
    for owner, prototypes in (
        ("BP01", ("BP01-001", "BP01-013")),
        ("BP10", ("BP10-020",)),
    ):
        prototype = read_tree(REPO / f"authored/rules/effects/{owner}.yaml")
        prototype["cards"] = {
            number: object_value(prototype["cards"])[number] for number in prototypes
        }
        target = root / f"rules/effects/{owner}.yaml"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(authoring.format_yaml(prototype))
    (root / "rules/effects-index.yaml").write_bytes(authoring.format_yaml(index))
    return root


@pytest.fixture(scope="session")
def workspace(
    repository_workspace: authoring.Workspace, authoring_inventory: Path
) -> authoring.Workspace:
    return replace(repository_workspace, authored=authoring_inventory)


@pytest.fixture
def output_root(tmp_path: Path, authoring_inventory: Path) -> Path:
    return Path(shutil.copytree(authoring_inventory, tmp_path / "authored"))


@pytest.fixture(scope="session")
def complete_repository_check(
    authoring_inputs: authoring.Inputs,
    tmp_path_factory: pytest.TempPathFactory,
    worker_id: str,
) -> Callable[[], RepositoryCheck]:
    root = tmp_path_factory.getbasetemp()
    if worker_id != "master":
        root = root.parent
    root /= "complete-authoring-check"
    # The writer proves byte equality before reusing the CLI validation of these inputs.
    return lambda: repository_check(root, authoring_inputs)
