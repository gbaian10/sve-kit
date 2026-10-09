"""Synthetic immutable checkouts using the public supported rule vocabulary only."""

import shutil
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.digital.name_policies.evaluate import NameOwner
from sve_carddb.domains.digital.name_policies.loader import load
from sve_carddb.domains.registry.storage import read_yaml

from .adoption_fixtures import REPO, commit, git
from .digital_link_import_fixtures import Fixture, catalogue_fixture, make_fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.digital.name_policies.loader import Snapshot

NAMES = "draft-i51-names-v1"
LINKS = "digital-name-links-v1"


def current(root: Path, identifier: str) -> dict[str, JsonValue]:
    return object_value(
        read_yaml(
            root
            / "authored/digital/policies"
            / ("names.yaml" if identifier == NAMES else "links.yaml")
        )
    )


def rewrite(root: Path, identifier: str, document: dict[str, JsonValue]) -> None:
    """Edit one purpose-bound policy without changing its permanent policy ID."""
    base = root / "authored/digital/policies"
    base.mkdir(parents=True, exist_ok=True)
    name = "names.yaml" if identifier == NAMES else "links.yaml"
    (base / name).write_bytes(canonical(document))


def copy_policies(root: Path, batches: list[JsonValue] | None = None) -> None:
    rewrite(
        root,
        LINKS,
        {
            "format": 2,
            "kind": "digital_name_policy",
            "policy_id": LINKS,
            "purpose": "links",
            "content": {
                "source_batches": batches
                if batches is not None
                else [{"batch_id": digest(b"synthetic catalogue batch")}],
                "excluded_names": [],
                "excluded_targets": [],
            },
        },
    )
    names = current(REPO, NAMES)
    names["note"] = "Synthetic current policy"
    object_value(names["content"]).update(excluded_names=[], name_overrides=[])
    rewrite(root, NAMES, names)


@dataclass(frozen=True)
class PolicyFixture:
    digital: Fixture
    authored: str

    @property
    def root(self) -> Path:
        return self.digital.root

    def snapshot(self) -> Snapshot:
        return load(self.root / "authored", self.authored)

    def owner(self, *, state: str = "known") -> NameOwner:
        config = object_value(parse(self.digital.build.configuration.encode()))
        batch = object_value(array(config["digital_link_sources"])[0])["batch_id"]
        ref = self.digital.jp.model_copy(update={"batch_id": batch})
        return NameOwner.model_validate_json(
            canonical(
                {
                    "kind": "face_revision",
                    "owner_id": "synthetic-revision",
                    "card_id": self.digital.card.id,
                    "face_id": self.digital.face.id,
                    "printing_id": self.digital.printing.id,
                    "state": state,
                    "name_ref": ref.model_dump(mode="json")
                    if state == "known"
                    else None,
                }
            )
        )


def make_policy_fixture(
    root: Path, *, translated_name: str = "合成測試名"
) -> PolicyFixture:
    def translate(data: dict[str, JsonValue], language: str) -> None:
        if language == "zh-tw":
            object_value(array(data["cards"])[0])["card_name"] = translated_name

    original = make_fixture(root)
    digital = catalogue_fixture(
        original, game="sv1", languages=("ja", "zh-tw"), transform=translate
    )
    configuration = object_value(parse(digital.build.configuration.encode()))
    configuration = {
        k: configuration[k]
        for k in ("catalog_registry", "digital_link_sources", "translation_recipes")
    }
    old_config = object_value(parse(original.build.configuration.encode()))
    configuration["digital_link_sources"] = sorted(
        [
            *array(configuration["digital_link_sources"]),
            *array(old_config["digital_link_sources"]),
        ],
        key=canonical,
    )
    copy_policies(root, array(configuration["digital_link_sources"]))
    authored = commit(root)
    build = BuildContext.from_inputs(authored, configuration)
    return PolicyFixture(replace(digital, program=authored, build=build), authored)


def copied_policy(fixture: PolicyFixture, root: Path) -> PolicyFixture:
    shutil.copytree(fixture.root, root)
    return replace(
        fixture,
        digital=replace(
            fixture.digital,
            root=root,
            store=root / fixture.digital.store.relative_to(fixture.root),
        ),
    )


def loader_repository(root: Path) -> tuple[Path, str]:
    copy_policies(root)
    git(root, "init")
    return root, commit(root)
