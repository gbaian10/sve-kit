"""Synthetic immutable checkouts using the public supported rule vocabulary only."""

import shutil
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.digital_name_policies.evaluate import NameOwner
from sve_carddb.digital_name_policies.loader import INDEX, load
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


def _link_files(purpose: str) -> tuple[dict[str, JsonValue], ...]:
    """Retain the adopted versioned links contract with synthetic provenance."""
    business = object_value(
        parse(
            (
                REPO / "carddb/tests/fixtures/digital-name-policy-legacy-business.json"
            ).read_bytes()
        )
    )
    content = object_value(business[purpose])
    assert purpose == "links"
    identifier = LINKS
    checksum = digest(b"synthetic policy evidence; no actual approval")
    revision = "a" * 40
    batches: list[JsonValue] = [{"batch_id": checksum}]
    registry: dict[str, JsonValue] = {
        "authored_revision": revision,
        "index_path": "authored/ids/index.yaml",
        "index_hash": checksum,
    }
    recipes: dict[str, JsonValue] = {
        "translation-" + provider + "-v1": {
            "version": "translation-" + provider + "-v1",
            "program_revision": revision,
            "code_path": "carddb/src/sve_carddb/translations/sources.py",
            "code_hash": checksum,
            "config": {"provider": provider},
            "config_hash": digest(canonical({"provider": provider})),
        }
        for provider in ("jp", "sv1", "svwb")
    }
    content["catalogue_pins"] = {
        "approved_coverage_claim": False,
        "candidate_counts": {},
        "count_replay_main_revision": revision,
        "extra_unicode_scan_hash": checksum,
        "parser_and_registry_configuration": {
            "catalog_registry": registry,
            "digital_link_sources": batches,
            "translation_recipes": recipes,
        },
        "private_name_list_hash": checksum,
        "r2_inventory_evidence_hash": checksum,
        "source_batches": batches,
    }
    content["proposed_clause_replacements"] = []
    if purpose == "links":
        content["registry_pins"] = {
            "revision": revision,
            "index_hash": checksum,
            "card_projection_evidence_hash": checksum,
            "source_replay_revision": revision,
        }
    approved = checksum
    document: dict[str, JsonValue] = {
        "digital_name_policy_format": 1,
        "kind": "digital_name_policy",
        "policy_id": identifier,
        "version": 1,
        "purpose": purpose,
        "approved_document_hash": approved,
        "projection_recipe": "approved-digital-name-document-v1",
        "content": content,
    }
    excluded: dict[str, JsonValue] = {
        "digital_name_exclusion_format": 1,
        "kind": "digital_name_initial_exclusions",
        "policy_id": identifier,
        "version": 1,
        "purpose": purpose,
        "approved_list_hash": object_value(content["exclusions"])[
            "initial_exclusions_hash"
        ],
        "entries": [],
    }
    return document, excluded


def copy_policies(root: Path) -> None:
    index: dict[str, JsonValue] = {
        "digital_name_policy_index_format": 2,
        "kind": "digital_name_policy_index",
        "policies": {},
    }
    for purpose, identifier in (("links", LINKS),):
        document, excluded = _link_files(purpose)
        base = root / "authored/digital-name-policies" / identifier
        base.mkdir(parents=True, exist_ok=True)
        exclusions = root / "authored/digital-name-exclusions" / identifier
        exclusions.mkdir(parents=True, exist_ok=True)
        (base / "001.policy.yaml").write_bytes(canonical(document))
        (exclusions / "001.yaml").write_bytes(canonical(excluded))
        object_value(index["policies"])[identifier] = [
            {
                "version": 1,
                "path": f"digital-name-policies/{identifier}/001.policy.yaml",
                "hash": digest(canonical(document)),
                "exclusions_path": f"digital-name-exclusions/{identifier}/001.yaml",
                "exclusions_hash": digest(canonical(excluded)),
                "predecessor": None,
            }
        ]
    current = object_value(
        read_yaml(REPO / "authored/digital-name-policies" / NAMES / "current.yaml")
    )
    current["note"] = "Synthetic current policy"
    object_value(current["content"]).update(excluded_names=[], name_overrides=[])
    base = root / "authored/digital-name-policies" / NAMES
    base.mkdir(parents=True)
    (base / "current.yaml").write_bytes(canonical(current))
    object_value(index["policies"])[NAMES] = {
        "path": f"digital-name-policies/{NAMES}/current.yaml",
        "hash": digest(canonical(current)),
    }
    (root / "authored" / INDEX).write_bytes(canonical(index))


def policy(root: Path, purpose: str) -> dict[str, JsonValue]:
    return object_value(
        read_yaml(
            root
            / "authored/digital-name-policies"
            / (NAMES if purpose == "names" else LINKS)
            / "001.policy.yaml"
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
    excluded: dict[str, JsonValue] | None = None,
) -> None:
    assert purpose == "links"
    identifier = LINKS
    base = root / "authored/digital-name-policies" / identifier
    document = document if document is not None else policy(root, purpose)
    excluded = excluded if excluded is not None else exclusion(root, purpose)
    for path, value in [
        (base / "001.policy.yaml", document),
        (root / "authored/digital-name-exclusions" / identifier / "001.yaml", excluded),
    ]:
        path.write_bytes(canonical(value))
    index_path = root / "authored" / INDEX
    index = object_value(read_yaml(index_path))
    entry = object_value(array(object_value(index["policies"])[identifier])[0])
    entry.update(
        hash=digest(canonical(document)),
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
    for purpose in ("links",):
        document = policy(root, purpose)
        document["approved_document_hash"] = digest(canonical(["synthetic", purpose]))
        content = object_value(document["content"])
        pins = object_value(content["catalogue_pins"])
        pins.update(
            count_replay_main_revision=digital.program,
            parser_and_registry_configuration=configuration,
            source_batches=configuration["digital_link_sources"],
        )
        if purpose == "links":
            registry = object_value(configuration["catalog_registry"])
            object_value(content["registry_pins"]).update(
                revision=registry["authored_revision"],
                index_hash=registry["index_hash"],
                source_replay_revision=digital.program,
            )
        rewrite(root, purpose, document)
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
