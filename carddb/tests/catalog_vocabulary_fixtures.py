"""One sealed invented bilingual field fixture with immutable catalog receipts."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext
from sve_carddb.manifest import Kind, Region
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.source_archive import seal_batch

from .adoption_fixtures import (
    CODE,
    REPO,
    Case,
    commit,
    dependency,
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
    "carddb/src/sve_carddb/catalog/adoption_sources.py",
    "carddb/src/sve_carddb/snapshot/values.py",
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
            {name: (self.case.repository / name).read_bytes() for name in RUNTIME},
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
    case: VocabularyCase,
    kind: str,
    code: str,
    mappings: list[dict[str, JsonValue]],
    *,
    active: bool = True,
) -> dict[str, JsonValue]:
    dependencies = {canonical(dependency("language", code="ja"))}
    for item in mappings:
        dependencies.add(canonical(dependency("language", code=str(item["lang"]))))
        dependencies.update(
            canonical(dependency("vocabulary", kind="special_kind", code=str(marker)))
            for marker in array(item["special_kinds"])
        )
    from sve_carddb.snapshot.values import parse  # ruff: ignore[import-outside-top-level] -- detached signed dependencies reuse canonical deduplication

    result = record(
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
        case.case.review,
        [parse(value) for value in sorted(dependencies)],
    )
    result["evidence"] = sorted(
        [
            {"source_ref": item["source_ref"], "role": "Synthetic complete field"}
            for item in mappings
        ],
        key=canonical,
    )
    return result


def save(case: VocabularyCase, records: list[dict[str, JsonValue]]) -> VocabularyCase:
    write(
        case.case.root,
        VOCABULARY_PATH,
        envelope(list[JsonValue](records), case.case.review),
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


def make_vocabulary_case(root: Path) -> VocabularyCase:  # ruff: ignore[too-many-locals,complex-structure] -- one shared bilingual archive and immutable recipe closure
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
    path = "carddb/src/sve_carddb/snapshot/values.py"
    recipe: dict[str, JsonValue] = {
        "version": "exact-json-v1",
        "program_revision": revision,
        "code_path": path,
        "code_hash": digest((REPO / path).read_bytes()),
        "config": {},
        "config_hash": digest(canonical({})),
    }
    context = BuildContext.from_inputs(
        revision,
        {name: (REPO / name).read_bytes() for name in RUNTIME},
        {"catalog_source_recipes": {"exact-json-v1": recipe}},
    )
    review: dict[str, JsonValue] = {
        "context": context.model_dump(mode="json"),
        "source_batches": [{"batch_id": batch.batch_id}],
    }
    for file in (case.root / "catalog-adoptions").rglob("*.yaml"):
        if file.name == "index.yaml":
            continue
        members = array(object_value(read_yaml(file))["records"])
        for value in members:
            object_value(object_value(value)["data"])["review_context_hash"] = digest(
                canonical(review)
            )
        write(
            case.root, file.relative_to(case.root).as_posix(), envelope(members, review)
        )
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
            vocabulary_record(result, "special_kind", "evolve", []),
            vocabulary_record(
                result,
                "type",
                "follower",
                [
                    mapping(result, region=region, markers=("evolve",))
                    for region in ("jp", "en")
                ],
            ),
            vocabulary_record(
                result,
                "class",
                "elf",
                [
                    mapping(result, region=region, kind="class")
                    for region in ("jp", "en")
                ],
            ),
        ],
    )


def current_vocabulary_case(case: VocabularyCase) -> VocabularyCase:
    """Replace synthetic receipt vocabulary with the native current entry."""
    includes: dict[str, JsonValue] = {}
    for path in (case.case.root / "catalog-adoptions").rglob("*.yaml"):
        if path.name == "index.yaml":
            continue
        relative = path.relative_to(case.case.root).as_posix()
        if "/vocabulary/" not in relative and "/languages/" not in relative:
            path.unlink()
            continue
        rows: list[JsonValue] = []
        for raw in array(object_value(read_yaml(path))["records"]):
            record = object_value(raw)
            data = object_value(record["data"])
            rows.append(
                {
                    "record_key": canonical([record["kind"], data["subject"]]).decode(),
                    "kind": record["kind"],
                    "data": {
                        "subject": data["subject"],
                        "value": data["value"],
                        "evidence": record["evidence"],
                    },
                    "origin": "project",
                    "low_confidence": False,
                    "note": "Synthetic current value",
                }
            )
        payload: dict[str, JsonValue] = {
            "catalog_adoption_format": 2,
            "kind": "catalog_adoption_shard",
            "records": sorted(rows, key=lambda r: str(object_value(r)["record_key"])),
        }
        path.write_bytes(canonical(payload))
        includes[relative] = digest(canonical(payload))
    write(
        case.case.root,
        "catalog-adoptions/index.yaml",
        {
            "catalog_adoption_format": 2,
            "kind": "catalog_adoption_index",
            "includes": dict(sorted(includes.items())),
        },
    )
    return replace(case, case=replace(case.case, revision=commit(case.case.repository)))
