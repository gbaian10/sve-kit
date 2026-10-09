"""Editable name rules reject invalid structure without an approval hash chain."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.domains.digital.name_policies.catalogue import catalogue
from sve_carddb.domains.digital.name_policies.loader import load
from sve_carddb.domains.registry.storage import read_yaml

from .adoption_fixtures import commit
from .name_application_fixtures import ApplicationCase, application_case
from .test_name_current_application import current_case

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def original(tmp_path_factory: pytest.TempPathFactory) -> ApplicationCase:
    return application_case(tmp_path_factory.mktemp("current-policy"))


def test_current_policy_comment_changes_no_rules(
    original: ApplicationCase,
    tmp_path: Path,
) -> None:
    case = current_case(original, tmp_path / "repo")
    first = catalogue(case.inputs.load().current_names[0], case.sources())
    path = case.inputs.root / "digital/policies/names.yaml"
    policy = object_value(read_yaml(path))
    policy["note"] = "修正說明。"
    path.write_bytes(canonical(policy))
    revision = commit(case.inputs.repository)
    changed = load(case.inputs.root, revision)
    second = catalogue(changed.current_names[0], case.sources())
    assert first.policy_hash == second.policy_hash
    assert first.names == second.names


@pytest.mark.parametrize("change", ["extra", "game", "duplicate_exclusion", "approval"])
def test_current_policy_refuses_invalid_fields(
    original: ApplicationCase,
    tmp_path: Path,
    change: str,
) -> None:
    case = current_case(original, tmp_path / "repo")
    path = case.inputs.root / "digital/policies/names.yaml"
    policy = object_value(read_yaml(path))
    content = object_value(policy["content"])
    if change == "extra":
        content["undeclared"] = True
    elif change == "game":
        content["game_priority"] = ["svwb", "sv1"]
    elif change == "duplicate_exclusion":
        content["excluded_names"] = [
            {
                "source_lang": "ja",
                "source_name_hash": digest(b"Synthetic card"),
                "reason": "Synthetic exclusion",
            }
        ] * 2
    else:
        policy["approval_receipt"] = "private"
    path.write_bytes(canonical(policy))
    revision = commit(case.inputs.repository)
    with pytest.raises(ValueError, match=r"^Invalid digital-name policy fields$"):
        load(case.inputs.root, revision)
