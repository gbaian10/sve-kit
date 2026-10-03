"""Shared sealed synthetic template definitions; no local private archive or Git identity."""

import shutil
from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameters.analysis import SAFE_INTEGER, VERSION_PARAMETERS
from sve_carddb.template_parameters.models import Schema, Slot
from sve_carddb.template_sources.models import Recipe
from sve_carddb.template_translations.loader import load_templates, payload
from sve_carddb.template_translations.models import DefinitionRecord
from sve_carddb.template_translations.sources import ORDINALS, TemplateSources

from .adoption_fixtures import commit, git
from .recognition_policy_fixtures import policy_git
from .recognition_replay_fixtures import recognition_term_source
from .translation_fixtures import envelope

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.template_translations.loader import Snapshot
    from sve_carddb.template_translations.sources import Reconstructed, SourceReplay

    from .recognition_replay_fixtures import SourceCase

__all__ = ("policy_git", "recognition_term_source")
INSTANT = "2026-10-02T00:00:00Z"
INVENTORY = "translations/template-sources/001.yaml"
DEFINITIONS = "translations/templates/definitions/001.yaml"
TRANSLATIONS = "translations/templates/translations/001.yaml"


def definition(member: Reconstructed, *, new: bool = False) -> dict[str, JsonValue]:
    if member.pending:
        raise ValueError("Template definition source has unresolved parameter roles")
    schema = Schema(
        slots=tuple(
            Slot(
                name=h.name,
                type=h.type or "uint",
                occurrences=(h.occurrence,),
                reference_kind=h.reference_kind,
                min=(1 if role in ORDINALS else 0) if h.type == "uint" else None,
                max=SAFE_INTEGER if h.type == "uint" else None,
            )
            for h, role in zip(member.hints, member.roles, strict=True)
        )
    )
    member.verify_schema(schema)
    identifier = member.candidate.legacy_id or "T" + "0" * 16
    record: dict[str, JsonValue] = {
        "record_key": canonical(["sentence_template", identifier]).decode(),
        "kind": "sentence_template",
        "filing_key": "definitions",
        "data": {
            "id": identifier,
            "inventory_id": member.entry.id,
            "source_span": member.candidate.source_span.model_dump(mode="json"),
            "source_lang": "ja",
            "normalizer_version": VERSION_PARAMETERS,
            "semantic_variant": "default",
            "parameter_schema": schema.model_dump(mode="json"),
            "content_hash": "sha256:" + "0" * 64,
            "supersedes_id": None,
        },
        "evidence": [],
    }
    checksum = digest(
        payload(member, DefinitionRecord.model_validate_json(canonical(record)))
    )
    data = object_value(record["data"])
    data["content_hash"] = checksum
    if new:
        data["id"] = "T" + checksum[7:23]
        record["record_key"] = canonical([record["kind"], data["id"]]).decode()
    return record


def translation(
    record: dict[str, JsonValue], *, revision: int = 1, text: str | None = None
) -> dict[str, JsonValue]:
    data = object_value(record["data"])
    if text is None:
        schema = Schema.model_validate_json(canonical(data["parameter_schema"]))
        text = "Synthetic translation " + " ".join(
            "{{" + s.name + "}}" for s in schema.slots
        )
    return {
        "record_key": canonical(
            ["template_translation", data["id"], "zh-Hant", revision]
        ).decode(),
        "kind": "template_translation",
        "filing_key": "translations",
        "data": {
            "template_id": data["id"],
            "lang": "zh-Hant",
            "revision": revision,
            "text": text,
            "origin": "machine",
            "model_review": {
                "translated_by": "Synthetic Translator v1",
                "reviewed_by": "Synthetic Reviewer v2",
                "reviewed_at": INSTANT,
                "text_hash": digest(text.encode()),
                "result": "agreed",
                "resolution": None,
            },
            "adoption_review": {
                "mode": "human",
                "policy": None,
                "initial_sample_decisions": [],
            },
        },
        "evidence": [],
    }


def shard(records: list[dict[str, JsonValue]]) -> dict[str, JsonValue]:
    value = envelope(records)
    decision = object_value(array(value["decisions"])[0])
    decision.update(state="sampled", policy_id="synthetic-template-human-v1")
    return value


def write(repository: Path, files: dict[str, JsonValue]) -> str:
    root = repository / "authored"
    index_file = root / "translations/index.yaml"
    index = object_value(parse(index_file.read_bytes()))
    for relative, value in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(canonical(value))
        mapping = object_value(
            index["inventories" if "template-sources/" in relative else "includes"]
        )
        mapping[relative] = digest(canonical(value))
    index_file.write_bytes(canonical(index))
    return commit(repository)


@dataclass(frozen=True)
class Case:
    source: SourceCase
    repository: Path
    prior: str
    revision: str
    pins: tuple[Recipe, ...]
    replay: SourceReplay
    files: dict[str, JsonValue]
    definitions: tuple[dict[str, JsonValue], ...]
    verified: Snapshot
    replay_sources: TemplateSources

    def fork(self, root: Path, *, published: bool = False) -> Path:
        """Change prospective receipts before publication rather than mutating signed shards."""
        git(
            self.repository,
            "clone",
            "--shared",
            "--no-checkout",
            str(self.repository),
            str(root),
        )
        git(root, "reset", "--hard", self.revision if published else self.prior)
        return root

    def sources(self) -> TemplateSources:
        """Prospective authored clones reuse the immutable code/source pins."""
        return self.replay_sources


@pytest.fixture(scope="module")
def intake_case(
    recognition_term_source: SourceCase, tmp_path_factory: pytest.TempPathFactory
) -> Case:
    source = recognition_term_source
    repository = tmp_path_factory.mktemp("template-intake") / "repository"
    shutil.copytree(source.repository, repository)
    prior = git(repository, "rev-parse", "HEAD")
    pins = tuple(
        sorted(
            (
                *array(source.recipe.config["source_recipes"]),
                source.recipe.model_dump(mode="json"),
            ),
            key=lambda r: str(object_value(r)["id"]),
        )
    )
    typed = tuple(Recipe.model_validate_json(canonical(p)) for p in pins)
    sources = TemplateSources(
        PinnedRepository(repository),
        {"test-store": source.store},
        main_revision=source.main,
        legacy_bytes=source.legacy,
        proposals=source.proposals,
    )
    replay = sources.reconstruct(typed)
    chosen = tuple(
        m for m in replay.entries if m.entry.role == "body" and not m.pending
    )
    assert len(chosen) == 2, [
        (m.normalized, m.pending) for m in replay.entries if m.entry.role == "body"
    ]
    definitions = tuple(definition(m) for m in chosen)
    files: dict[str, JsonValue] = {
        INVENTORY: {
            "template_source_format": 1,
            "kind": "template_source_inventory",
            "recipes": [p.model_dump(mode="json") for p in typed],
            "entries": [
                m.entry.model_dump(mode="json")
                for m in sorted(replay.entries, key=lambda m: m.entry.id)
            ],
        },
        DEFINITIONS: shard(list(definitions)),
        TRANSLATIONS: shard([translation(r) for r in definitions]),
    }
    revision = write(repository, files)
    verified = load_templates(PinnedRepository(repository), revision, sources)
    return Case(
        source,
        repository,
        prior,
        revision,
        typed,
        replay,
        files,
        definitions,
        verified,
        sources,
    )
