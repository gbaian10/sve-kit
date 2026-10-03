"""Synthetic immutable checkouts using the public supported rule vocabulary only."""

import shutil
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.digital_name_policies.evaluate import NameOwner
from sve_carddb.digital_name_policies.loader import DIRECTORIES, INDEX, load
from sve_carddb.digital_name_policies.runtime import RUNTIME
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse

from .adoption_fixtures import REPO, commit, git
from .digital_link_import_fixtures import Fixture, catalogue_fixture, make_fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.digital_name_policies.loader import Snapshot

NAMES = "draft-i51-names-v1"
LINKS = "digital-name-links-v1"


def copy_policies(root: Path) -> None:
    for directory in DIRECTORIES:
        shutil.copytree(REPO / "authored" / directory, root / "authored" / directory)


def policy(root: Path, purpose: str) -> dict[str, JsonValue]:
    return object_value(
        read_yaml(
            root
            / "authored/digital-name-policies"
            / (NAMES if purpose == "names" else LINKS)
            / "001.policy.yaml"
        )
    )


def receipt(root: Path, purpose: str) -> dict[str, JsonValue]:
    return object_value(
        read_yaml(
            root
            / "authored/digital-name-policies"
            / (NAMES if purpose == "names" else LINKS)
            / "001.approval.yaml"
        )
    )


def exclusion(root: Path, purpose: str) -> dict[str, JsonValue]:
    return object_value(
        read_yaml(
            root
            / "authored/digital-name-exclusions"
            / (NAMES if purpose == "names" else LINKS)
            / "001.yaml"
        )
    )


def rewrite(
    root: Path,
    purpose: str,
    document: dict[str, JsonValue] | None = None,
    approval: dict[str, JsonValue] | None = None,
    excluded: dict[str, JsonValue] | None = None,
    *,
    close: bool = True,
) -> None:
    identifier = NAMES if purpose == "names" else LINKS
    base = root / "authored/digital-name-policies" / identifier
    document = document if document is not None else policy(root, purpose)
    approval = approval if approval is not None else receipt(root, purpose)
    excluded = excluded if excluded is not None else exclusion(root, purpose)
    if close:
        approval.update(
            policy_hash=digest(canonical(document)),
            approved_document_hash=document["approved_document_hash"],
            initial_exclusions_hash=digest(canonical(excluded)),
        )
        for event in array(approval["approval_events"]):
            item = object_value(event)
            if item["kind"] == "page_button":
                object_value(item["value"])["policy_hash"] = document[
                    "approved_document_hash"
                ]
        object_value(approval["evidence_hashes"])[
            purpose + "-policy.canonical.json"
        ] = document["approved_document_hash"]
    for path, value in [
        (base / "001.policy.yaml", document),
        (base / "001.approval.yaml", approval),
        (root / "authored/digital-name-exclusions" / identifier / "001.yaml", excluded),
    ]:
        path.write_bytes(canonical(value))
    index_path = root / "authored" / INDEX
    index = object_value(read_yaml(index_path))
    entry = object_value(array(object_value(index["policies"])[identifier])[0])
    entry.update(
        hash=digest(canonical(document)),
        approval_receipt_hash=digest(canonical(approval)),
        exclusions_hash=digest(canonical(excluded)),
    )
    index_path.write_bytes(canonical(index))


@dataclass(frozen=True)
class PolicyFixture:
    digital: Fixture
    authored: str

    @property
    def root(self) -> Path:
        return self.digital.root

    def snapshot(self) -> Snapshot:
        return load(self.root / "authored", self.root, self.authored)

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


def make_policy_fixture(root: Path) -> PolicyFixture:
    original = make_fixture(root)
    digital = catalogue_fixture(original, game="sv1", languages=("ja", "zh-tw"))
    copy_policies(root)
    runtime_files = RUNTIME
    for name in runtime_files:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(REPO / name, target)
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
    for purpose in ("names", "links"):
        document = policy(root, purpose)
        document["approved_document_hash"] = digest(canonical(["synthetic", purpose]))
        content = object_value(document["content"])
        pins = object_value(content["catalogue_pins"])
        pins.update(
            count_replay_main_revision=digital.program,
            parser_and_registry_configuration=configuration,
            source_batches=configuration["digital_link_sources"],
            store_id="test-store",
        )
        if purpose == "links":
            registry = object_value(configuration["catalog_registry"])
            object_value(content["registry_pins"]).update(
                revision=registry["authored_revision"],
                index_hash=registry["index_hash"],
                source_replay_revision=digital.program,
            )
        approval = receipt(root, purpose)
        approval["disclosed_changes"] = []
        button = object_value(array(approval["approval_events"])[0])
        approval["approval_events"] = [button]
        approval["note"] = "Synthetic approval for boundary tests; never adopted data."
        rewrite(root, purpose, document, approval)
    authored = commit(root)
    dependencies = {n: (root / n).read_bytes() for n in (*RUNTIME, *runtime_files)}
    build = BuildContext.from_inputs(authored, dependencies, configuration)
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
