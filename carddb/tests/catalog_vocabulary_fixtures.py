"""One sealed invented bilingual field fixture with immutable catalog receipts."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.core.provenance import BuildContext
from sve_carddb.core.regions import SourceRegion as Region
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch

from .adoption_fixtures import (
    CODE,
    REPO,
    Case,
    commit,
    envelope,
    index,
    make_case,
    record,
    source_configuration,
    write,
)
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

RUNTIME = (
    CODE,
    "carddb/uv.lock",
    "carddb/pyproject.toml",
    "carddb/src/sve_carddb/domains/catalog/adoption_sources.py",
    "carddb/src/sve_carddb/core/json.py",
)
VOCABULARY_PATH = "catalog-adoptions/vocabulary/shared/001.yaml"


@dataclass(frozen=True)
class VocabularyCase:
    case: Case
    archive: Path
    references: dict[tuple[str, str, int], dict[str, JsonValue]]

    def build(self) -> BuildContext:
        return BuildContext.from_inputs(
            self.case.revision,
            source_configuration(self.case, self.case.inputs().configuration()),
        )


def mapping(
    case: VocabularyCase,
    *,
    region: str = "jp",
    kind: str = "type",
    face: int = 0,
    markers: tuple[str, ...] = (),
) -> dict[str, JsonValue]:
    raw = (
        "-" if face == 1 else "Synthetic type" if kind == "type" else "Synthetic class"
    )
    return {
        "region": region,
        "lang": "ja" if region == "jp" else "en",
        "raw": raw,
        "source_ref": case.references[region, kind, face],
        "special_kinds": list[JsonValue](markers),
    }


def vocabulary_record(
    kind: str,
    code: str,
    mappings: list[dict[str, JsonValue]],
    *,
    active: bool = True,
) -> dict[str, JsonValue]:
    return record(
        "vocabulary_adoption",
        {"kind": kind, "code": code},
        {
            "label": {
                "kind": "authored",
                "lang": "ja",
                "text": "Synthetic label " + code,
            },
            "raw_mappings": sorted(list[JsonValue](mappings), key=canonical),
            "active": active,
        },
    )


def save(case: VocabularyCase, records: list[dict[str, JsonValue]]) -> VocabularyCase:
    write(
        case.case.root,
        VOCABULARY_PATH,
        envelope(list[JsonValue](records)),
    )
    index(case.case.root)
    return replace(case, case=replace(case.case, revision=commit(case.case.repository)))


def records(case: VocabularyCase) -> list[dict[str, JsonValue]]:
    return [
        object_value(value)
        for value in array(
            object_value(read_yaml(case.case.root / VOCABULARY_PATH))["records"]
        )
    ]


def make_vocabulary_case(root: Path) -> VocabularyCase:
    store = _store(root / "archive")
    versions = {}
    for region in ("jp", "en"):
        raw = canonical(
            {
                "faces": [
                    {
                        "card_type": value,
                        "card_class": "-" if value == "-" else "Synthetic class",
                        "info": {
                            "Card Type": value,
                            "Class": "-" if value == "-" else "Synthetic class",
                        },
                    }
                    for value in ("Synthetic type", "-", "Synthetic type")
                ]
            }
        )
        resource = replace(
            _resource(
                "https://example.invalid/" + region,
                "raw/" + region + ".json",
                raw,
                Kind.CARD,
            ),
            region=Region(region),
            content_type="application/json",
        )
        _put(store, resource, raw)
    batch = seal_batch(store)
    for entry in batch.inventory.current:
        versions["jp" if entry.url.endswith("jp") else "en"] = entry.source_version_id
    case = make_case(root / "repository")
    for name in RUNTIME:
        target = case.repository / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / name).read_bytes())
    revision = commit(case.repository)
    recipe: dict[str, JsonValue] = {"version": "exact-json-v1", "config": {}}
    context = BuildContext.from_inputs(
        revision,
        {"catalog_source_recipes": {"exact-json-v1": recipe}},
    )
    review: dict[str, JsonValue] = {
        "context": context.model_dump(mode="json"),
        "source_batches": [{"batch_id": batch.batch_id}],
    }
    references: dict[tuple[str, str, int], dict[str, JsonValue]] = {}
    for region in ("jp", "en"):
        for kind in ("type", "class"):
            for face in range(3):
                field = "card_type" if kind == "type" else "card_class"
                if region == "en":
                    field = "info/" + ("Card Type" if kind == "type" else "Class")
                text = (
                    "-"
                    if face == 1
                    else "Synthetic type"
                    if kind == "type"
                    else "Synthetic class"
                )
                references[region, kind, face] = {
                    "batch_id": batch.batch_id,
                    "source_version_id": versions[region],
                    "parser": "exact-json-v1",
                    "locator": f"/faces/{face}/{field}",
                    "text_hash": digest(text.encode()),
                }
    result = VocabularyCase(replace(case, review=review), store.root, references)
    return save(
        result,
        [
            vocabulary_record("special_kind", "evolve", []),
            vocabulary_record(
                "type",
                "follower",
                [
                    mapping(result, region=region, markers=("evolve",))
                    for region in ("jp", "en")
                ],
            ),
            vocabulary_record(
                "class",
                "elf",
                [
                    mapping(result, region=region, kind="class")
                    for region in ("jp", "en")
                ],
            ),
        ],
    )
