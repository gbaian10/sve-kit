"""Export the public logical projection to immutable candidate transport blobs."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING
from urllib.parse import quote

from pydantic import JsonValue

from sve_carddb.contracts.profiles import MEDIA
from sve_carddb.contracts.profiles import profile as profile_for
from sve_carddb.contracts.snapshot import (
    columns,
    definition,
    row_type,
    tables,
    validate,
)
from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.export.name_annotations import compact_names, whole_name
from sve_carddb.export.project.source import json_list
from sve_carddb.export.reader import read_snapshot
from sve_carddb.export.semantics import row_key
from sve_carddb.export.transport.compression import Blob, Brotli, compress, recipe
from sve_carddb.export.transport.layout import Group, Layout, Ownership, references
from sve_carddb.export.transport.wire import container, encode

if TYPE_CHECKING:
    from sve_carddb.export.project import Projection

type Record = dict[str, JsonValue]

__all__ = ["Batch", "Blob", "Brotli", "Ownership", "Snapshot", "export_snapshot"]


@dataclass(frozen=True)
class Batch:
    """Candidate batch metadata is kept outside reusable payload bytes."""

    data_version: str
    published_at: str
    regions: tuple[str, ...]


@dataclass(frozen=True)
class Snapshot:
    """Complete in-memory candidate; the preview/publisher owns filesystem writes."""

    manifest: Record
    payloads: dict[str, Blob]
    text_all: Blob
    compression_recipe: dict[str, str | int | None]

    def verify(self, projection: Projection) -> None:
        """Require the independent reader's complete join to equal the logical view."""
        joined = read_snapshot(
            self.manifest, {key: blob.raw for key, blob in self.payloads.items()}
        )
        if joined != projection.tables:
            raise ValueError("Export join differs from public logical projection")

    def assert_identical(self, other: Snapshot) -> None:
        """Compare exact raw and compressed bytes, including the alternative union."""
        if (
            self.payloads != other.payloads
            or self.text_all != other.text_all
            or self.compression_recipe != other.compression_recipe
        ):
            raise ValueError("Clean exports differ in payload or compression bytes")


def _key(row: Record, fields: list[str]) -> tuple[tuple[int, int | str | bytes], ...]:
    return tuple(row_key(row[field]) for field in fields)


def _ordered(table: str, rows: list[Record]) -> list[Record]:
    fields = [string(item) for item in array(definition(table)["x-primary-key"])]
    keys = [_key(row, fields) for row in rows]
    if len(set(keys)) != len(keys):
        raise ValueError("Duplicate public primary key")
    return sorted(rows, key=lambda row: _key(row, fields))


def _partition(layout: Layout) -> dict[Group, dict[str, list[Record]]]:
    result: dict[Group, dict[str, list[Record]]] = {}
    text_ids: set[str] = set()
    translation_ids: set[str] = set()
    mapping = object_value(
        definition("Container", layout.profile.version)["x-fragments"]
    )
    for table, rows in layout.view.items():
        if table in {
            "text_unit",
            "translation",
            "field_annotation",
            "annotation_set",
            "annotation_concept",
        }:
            continue
        for row in _ordered(table, rows):
            part = (
                "history"
                if table == "face_revision" and row["id"] not in layout.display
                else "bootstrap"
                if "bootstrap" in object_value(mapping[table])
                else "detail"
            )
            group = layout.group(table, row, part)
            result.setdefault(group, {}).setdefault(table, []).append(row)
            if part == "bootstrap":
                texts, translations = references(layout.record(table, row, part, 0))
                text_ids |= texts
                translation_ids |= translations
    _annotation_partition(layout, result, text_ids, translation_ids)
    for table, selected in (("text_unit", text_ids), ("translation", translation_ids)):
        for row in _ordered(table, layout.view[table]):
            part = "bootstrap" if row["id"] in selected else "detail"
            result.setdefault(layout.group(table, row, part), {}).setdefault(
                table, []
            ).append(row)
    return result


def _annotation_partition(
    layout: Layout,
    result: dict[Group, dict[str, list[Record]]],
    text_ids: set[str],
    translation_ids: set[str],
) -> None:
    annotation_ids: set[str] = set()
    for row in _ordered("field_annotation", layout.view["field_annotation"]):
        part = layout.field_partition(row)
        group = layout.group("field_annotation", row, part)
        result.setdefault(group, {}).setdefault("field_annotation", []).append(row)
        if part == "bootstrap":
            annotation_ids.add(string(row["annotation_set_id"]))
    for row in layout.view["translation"]:
        if row["id"] in translation_ids:
            texts, _ = references(row)
            text_ids |= texts
            if row["annotation_set_id"] is not None:
                annotation_ids.add(string(row["annotation_set_id"]))
    text_rows = {string(row["id"]): row for row in layout.view["text_unit"]}
    annotation_ids |= {
        string(
            whole_name(
                text_rows[string(row["name_unit_id"])], string(row["name_concept_id"])
            )["id"]
        )
        for row in layout.view["face_revision"]
        if row["id"] in layout.display and row["name_concept_id"] is not None
    }
    concept_ids: set[str] = {
        string(row["name_concept_id"])
        for row in layout.view["face_revision"]
        if row["id"] in layout.display and row["name_concept_id"] is not None
    }
    for row in _ordered("annotation_set", layout.view["annotation_set"]):
        part = "bootstrap" if row["id"] in annotation_ids else "detail"
        group = layout.group("annotation_set", row, part)
        result.setdefault(group, {}).setdefault("annotation_set", []).append(row)
        if part == "bootstrap":
            text_ids.add(string(row["text_unit_id"]))
            concept_ids |= _concept_references(row)
    for row in _ordered("annotation_concept", layout.view["annotation_concept"]):
        part = "bootstrap" if row["id"] in concept_ids else "detail"
        group = layout.group("annotation_concept", row, part)
        result.setdefault(group, {}).setdefault("annotation_concept", []).append(row)


def _reference(file: Record) -> Record:
    return {"key": file["key"], "sha256": file["sha256"]}


def _description(blob: Blob) -> Record:
    hashed = digest(blob.raw)
    return {
        "path": "snapshots/blobs/" + hashed[7:] + ".json",
        "sha256": hashed,
        "bytes": len(blob.raw),
        "compressed_bytes": {
            "br": None if blob.br is None else len(blob.br),
            "gzip": len(blob.gzip),
        },
    }


class _Files:
    def __init__(self, brotli: Brotli | None) -> None:
        self.brotli = brotli
        self.files: dict[str, Record] = {}
        self.payloads: dict[str, Blob] = {}
        self.values: dict[str, JsonValue] = {}

    def add(
        self, key: str, role: str, value: Record, dependencies: list[JsonValue]
    ) -> Record:
        """Seal bytes before allowing other shards to bind their base hash."""
        if key in self.files:
            raise ValueError("Duplicate logical file key")
        blob = compress(canonical(value), self.brotli)
        counts: list[JsonValue] = [
            {
                "table": table,
                **{name: f[name] for name in ("owner", "bucket", "partition")},
                "count": len(array(f["rows"])),
            }
            for table, fragments in object_value(value.get("tables", {})).items()
            for fragment in array(fragments)
            for f in (object_value(fragment),)
        ]
        file = _description(blob) | {
            "key": key,
            "role": role,
            "row_counts": counts,
            "dependencies": dependencies,
        }
        self.files[key] = file
        self.payloads[key] = blob
        self.values[key] = value
        return file


def _file_key(group: Group) -> str:
    return "/".join(
        (
            group.role,
            group.partition,
            group.kind,
            quote(group.identifier, safe="") or "global",
            str(group.bucket),
        )
    )


def _fragments(
    layout: Layout, group: Group, records: dict[str, list[Record]], base: Record | None
) -> Record:
    fragments: Record = {}
    for table, rows in sorted(records.items()):
        name = row_type(table, group.partition, layout.profile.version)
        fragments[table] = [
            {
                "owner": group.owner,
                "bucket": group.bucket,
                "partition": group.partition,
                "base": None
                if base is None
                else {
                    "file": _reference(base),
                    "table": table,
                    "owner": group.owner,
                    "bucket": group.bucket,
                    "partition": "bootstrap",
                },
                "columns": list(columns(name, layout.profile.version)),
                "rows": [
                    encode(
                        name,
                        layout.record(table, row, group.partition, index),
                        layout.profile.version,
                    )
                    for index, row in enumerate(rows)
                ],
            }
        ]
    return container(fragments, layout.profile.version)


def _dependencies(
    table: str,
    fragment: Record,
    bootstrap_refs: dict[str, dict[str, Record]] | None,
) -> list[Record]:
    refs: list[Record] = []
    if table == "printing_image":
        if bootstrap_refs is None:
            raise ValueError("Media requires bootstrap locations")
        for raw_row in array(fragment["rows"]):
            row = array(raw_row)
            refs.extend(
                bootstrap_refs[field][string(row[index])]
                for field, index in (("printing", 0), ("face", 1))
            )
    if fragment["base"] is not None:
        refs.append(object_value(object_value(fragment["base"])["file"]))
    return refs


def _seal_bands(
    layout: Layout,
    files: _Files,
    pieces: list[tuple[Group, Record]],
    config: Record,
    bootstrap_refs: dict[str, dict[str, Record]] | None = None,
) -> dict[Group, Record]:
    bands: dict[Group, list[tuple[Group, Record]]] = {}
    for group, value in pieces:
        width = layout.profile.width(
            group.role, group.partition, group.kind, group.identifier
        )
        band = replace(group, bucket=group.bucket // width)
        bands.setdefault(band, []).append((group, value))
    locations: dict[Group, Record] = {}
    for band, entries in sorted(bands.items()):
        fragments: dict[str, list[JsonValue]] = {}
        dependencies: dict[str, Record] = {}
        if band.role == "images":
            dependencies[string(config["key"])] = _reference(config)
        for _, value in entries:
            for table, raw in object_value(value["tables"]).items():
                for fragment in array(raw):
                    fragments.setdefault(table, []).append(fragment)
                    for ref in _dependencies(
                        table, object_value(fragment), bootstrap_refs
                    ):
                        dependencies[string(ref["key"])] = ref
        tables_value: Record = {
            table: sorted(rows, key=lambda f: integer(object_value(f)["bucket"]))
            for table, rows in sorted(fragments.items())
        }
        key = _file_key(band).rsplit("/", 1)[0] + "/band/" + str(band.bucket)
        file = files.add(
            key,
            band.role,
            container(tables_value, layout.profile.version),
            [dependencies[k] for k in sorted(dependencies)],
        )
        for group, _ in entries:
            locations[group] = file
    return locations


def _add_bands(
    layout: Layout,
    files: _Files,
    groups: dict[Group, dict[str, list[Record]]],
    config: Record,
) -> None:
    bootstrap = [
        (group, _fragments(layout, group, records, None))
        for group, records in groups.items()
        if group.role == "bootstrap"
    ]
    bases = _seal_bands(layout, files, bootstrap, config)
    bootstrap_refs: dict[str, dict[str, Record]] = {"printing": {}, "face": {}}
    for group, records in groups.items():
        if group.role == "bootstrap":
            for table in ("printing", "face"):
                for row in records.get(table, []):
                    bootstrap_refs[table][string(row["id"])] = _reference(bases[group])
    remaining = [
        (group, _fragments(layout, group, records, None))
        for group, records in groups.items()
        if group.role != "bootstrap"
    ]
    for group, records in groups.items():
        if group.role != "bootstrap":
            continue
        split = {
            table: rows
            for table, rows in records.items()
            if table in {"printing", "face_revision"}
        }
        if split:
            detail = replace(group, role="text", partition="detail")
            remaining.append((detail, _fragments(layout, detail, split, bases[group])))
    _seal_bands(layout, files, remaining, config, bootstrap_refs)


def export_snapshot(
    projection: Projection,
    ownership: Ownership,
    batch: Batch,
    *,
    brotli: Brotli | None = None,
    format_version: str = MEDIA,
) -> Snapshot:
    """Export an explicitly selected fixed wire profile and verify the independent join."""
    if not batch.data_version.startswith("preview-"):
        raise ValueError("Candidate exporter requires a preview data version")
    if set(projection.tables) != set(tables()):
        raise ValueError("Exact public collection whitelist required")
    selected = profile_for(format_version)
    properties = object_value(definition("Manifest", format_version)["properties"])
    profile = object_value(object_value(properties["partitioning"])["properties"])
    count = integer(object_value(profile["bucket_count"])["const"])
    if any(
        row["region"] not in batch.regions
        for table in ("printing", "face_revision", "product")
        for row in projection.tables[table]
    ):
        raise ValueError("Public region is outside the explicit manifest scope")
    layout = Layout(compact_names(projection.tables), ownership, count, format_version)
    files = _Files(brotli)
    config = files.add(
        "config", "config", projection.config | {"format_version": format_version}, []
    )
    files.add(
        "programs", "programs", {"format_version": format_version, "entries": []}, []
    )
    groups = _partition(layout)
    _add_bands(layout, files, groups, config)
    contains: list[JsonValue] = [
        _reference(file)
        for key, file in sorted(files.files.items())
        if files.files[key]["role"] in {"bootstrap", "text", "config"}
    ]
    union = compress(
        canonical(
            {
                "format_version": format_version,
                "members": [
                    object_value(ref)
                    | {"payload": files.values[string(object_value(ref)["key"])]}
                    for ref in contains
                ],
            }
        ),
        brotli,
    )
    languages = json_list(
        sorted(
            {
                string(object_value(item)["code"])
                for item in array(projection.config["languages"])
            }
        )
    )
    partitioning: Record = {"algorithm": "sha256-mod-v1", "bucket_count": count}
    manifest: Record = projection.metadata | {
        "format_version": format_version,
        "data_version": batch.data_version,
        "published_at": batch.published_at,
        "regions": list(batch.regions),
        "languages": languages,
        "min_reader_version": format_version,
        "required_capabilities": list(selected.capabilities),
        "files": [files.files[key] for key in sorted(files.files)],
        "config_ref": _reference(config),
        "text_all": _description(union) | {"contains": contains},
        "partitioning": partitioning,
        "changes_ref": None,
    }
    validate("Manifest", manifest, format_version)
    result = Snapshot(manifest, files.payloads, union, recipe(brotli))
    result.verify(projection)
    return result


def _concept_references(row: Record) -> set[str]:
    result: set[str] = set()
    for raw in array(row["occurrences"]):
        reference = object_value(object_value(raw)["reference"])
        if reference["kind"] != "vocabulary":
            result.add(
                string(
                    reference["term_id"]
                    if reference["kind"] == "card_name"
                    else reference["key"]
                )
            )
    return result
