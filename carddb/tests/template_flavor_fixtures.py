"""One complete synthetic JP flavor/identity closure per module; no private data."""

import shutil
from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.catalog.adoption_models import Batch
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.manifest import Kind
from sve_carddb.registry.build import build as build_registry
from sve_carddb.registry.inputs import Mapping
from sve_carddb.registry.review import Inputs, Receipt
from sve_carddb.registry.storage import plan_files, read_yaml, write_files
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_translations.flavor_models import FlavorInputs
from sve_carddb.template_translations.flavor_pins import recipes
from sve_carddb.template_translations.loader import payload
from sve_carddb.template_translations.models import DefinitionRecord
from sve_carddb.template_translations.sources import SourceReplay, TemplateSources
from sve_carddb.translations.models import IdentityBasis

from .adoption_fixtures import commit, git
from .recognition_policy_fixtures import RUNTIME
from .template_intake_fixtures import (
    DEFINITIONS,
    INVENTORY,
    TRANSLATIONS,
    shard,
    translation,
)
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.template_sources.models import Recipe


def definition(member: object) -> dict[str, JsonValue]:
    """Use only a reconstructed zero-slot flavor member, never synthetic ID guesses."""
    from sve_carddb.template_translations.sources import Reconstructed  # ruff: ignore[import-outside-top-level] -- retain the helper's runtime boundary

    assert isinstance(member, Reconstructed)
    record: dict[str, JsonValue] = {
        "record_key": "pending",
        "kind": "sentence_template",
        "filing_key": "definitions",
        "data": {
            "id": "T" + "0" * 16,
            "inventory_id": member.entry.id,
            "source_span": member.candidate.source_span.model_dump(mode="json"),
            "source_lang": "ja",
            "normalizer_version": "flavor-exact-v1",
            "semantic_variant": "default",
            "parameter_schema": {"format": 1, "slots": []},
            "content_hash": "sha256:" + "0" * 64,
            "supersedes_id": None,
        },
        "evidence": [
            {
                "role": "synthetic flavor",
                "source_ref": member.entry.source_ref.model_dump(mode="json"),
            }
        ],
    }
    typed = DefinitionRecord.model_validate_json(canonical(record))
    checksum = digest(payload(member, typed))
    data = dict(typed.data.model_dump(mode="json"))
    data.update(id="T" + checksum[7:23], content_hash=checksum)
    record["data"] = data
    record["record_key"] = canonical(["sentence_template", data["id"]]).decode()
    return record


@dataclass(frozen=True)
class Case:
    repository: Path
    store: Path
    batch: str
    prior: str
    pins: tuple[Recipe, ...]
    basis: IdentityBasis
    replay: SourceReplay
    sources: TemplateSources
    files: dict[str, JsonValue]

    def fork(self, path: Path) -> Path:
        git(
            self.repository,
            "clone",
            "--shared",
            "--no-checkout",
            str(self.repository),
            str(path),
        )
        git(path, "reset", "--hard", self.prior)
        return path


@pytest.fixture(scope="module")
def flavor_case(tmp_path_factory: pytest.TempPathFactory) -> Case:  # ruff: ignore[too-many-locals] -- one module baseline shares all source and identity setup
    folder = tmp_path_factory.mktemp("flavor-intake")
    repository = folder / "repository"
    repository.mkdir()
    shutil.copytree(
        RUNTIME / "carddb/src",
        repository / "carddb/src",
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "_version.py"),
    )
    for name in ("uv.lock", "pyproject.toml"):
        shutil.copyfile(RUNTIME / "carddb" / name, repository / "carddb" / name)
    git(repository, "init", "-b", "main")
    store = _store(folder / "sources")
    jp = {}
    for index, paragraph in enumerate(
        (
            "甲２<br>（乙）<br><br>丙🌱",
            "甲２<br>（乙）<br><br>丙🌱",
            "甲３『乙』",
            None,
            "",
        ),
        1,
    ):
        number = f"SYN-{index:03}"
        raw = RAW.replace(
            b'<div class="speech"></div>',
            b""
            if paragraph is None
            else ('<div class="speech">' + paragraph + "</div>").encode(),
        )
        _put(
            store,
            _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            raw,
        )
        jp[number] = legacy_projection(extract_card(raw, number=number))
    batch = seal_batch(store)
    inputs = Inputs(
        jp=jp,
        en={},
        mapping=Mapping(targets={}, original_art=set(), reskins={}),
        receipt=Receipt(
            policy="identity-init-2026-09-28-v1",
            reviewed_by="Synthetic human",
            reviewed_on="2026-09-28",
            input_hashes={"jp": digest(b"synthetic flavor")},
            separate_groups={"jp:" + number: number for number in jp},
        ),
    )
    write_files(
        plan_files(
            repository / "authored",
            build_registry(inputs, {}),
            "Synthetic human",
            "2026-09-28",
        )
    )
    index_file = repository / "authored/translations/index.yaml"
    index_file.parent.mkdir(parents=True)
    index_file.write_bytes(
        canonical(
            {
                "translation_authored_format": 1,
                "kind": "translation_index",
                "includes": {},
                "inventories": {},
            }
        )
    )
    prior = commit(repository)
    basis = IdentityBasis(
        authored_revision=prior,
        registry_index_hash=digest(
            canonical(read_yaml(repository / "authored/ids/index.yaml"))
        ),
        transition_index_hash=None,
    )
    pins = recipes(PinnedRepository(repository), prior)
    sources = TemplateSources(
        PinnedRepository(repository),
        {store.store_id: store.root},
        main_revision=prior,
        legacy_bytes=b"",
        flavor=FlavorInputs(
            source_batch=Batch(store_id=store.store_id, batch_id=batch.batch_id),
            identity_basis=basis,
            identity_batches=(Batch(store_id=store.store_id, batch_id=batch.batch_id),),
        ),
    )
    replay = sources.reconstruct(pins)
    representatives = {m.normalized: m for m in replay.entries}
    definitions = [definition(m) for m in representatives.values()]
    files: dict[str, JsonValue] = {
        INVENTORY: {
            "template_source_format": 1,
            "kind": "template_source_inventory",
            "recipes": [r.model_dump(mode="json") for r in pins],
            "entries": [m.entry.model_dump(mode="json") for m in replay.entries],
        },
        DEFINITIONS: shard(definitions),
        TRANSLATIONS: shard(
            [translation(r, text="合成段落。\n\n另行。") for r in definitions]
        ),
    }
    return Case(
        repository,
        store.root,
        batch.batch_id,
        prior,
        pins,
        basis,
        replay,
        sources,
        files,
    )
