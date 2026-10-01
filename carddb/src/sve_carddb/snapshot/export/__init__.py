"""Export the public logical projection to immutable candidate transport blobs."""

from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import quote

from pydantic import JsonValue

from sve_carddb.snapshot.contract import columns, definition, row_type, tables, validate
from sve_carddb.snapshot.export.compression import Blob, Brotli, compress, recipe
from sve_carddb.snapshot.export.layout import Group, Layout, Ownership, references
from sve_carddb.snapshot.export.wire import container, encode
from sve_carddb.snapshot.project.source import json_list
from sve_carddb.snapshot.reader import read_snapshot
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    string,
)

if TYPE_CHECKING:
    from sve_carddb.snapshot.project import Projection

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


def _key(row: Record, fields: list[str]) -> tuple[tuple[int, int | str], ...]:
    return tuple(
        (0, 0)
        if row[field] is None
        else (1, string(row[field]))
        if isinstance(row[field], str)
        else (2, integer(row[field]))
        for field in fields
    )


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
    mapping = object_value(definition("Container")["x-fragments"])
    for table, rows in layout.view.items():
        if table in {"text_unit", "translation"}:
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
    for row in layout.view["translation"]:
        if row["id"] in translation_ids:
            texts, _ = references(row)
            text_ids |= texts
    for table, selected in (("text_unit", text_ids), ("translation", translation_ids)):
        for row in _ordered(table, layout.view[table]):
            part = "bootstrap" if row["id"] in selected else "detail"
            result.setdefault(layout.group(table, row, part), {}).setdefault(
                table, []
            ).append(row)
    return result


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
        name = row_type(table, group.partition)
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
                "columns": list(columns(name)),
                "rows": [
                    encode(name, layout.record(table, row, group.partition, index))
                    for index, row in enumerate(rows)
                ],
            }
        ]
    return container(fragments)


def _add_groups(
    layout: Layout,
    files: _Files,
    groups: dict[Group, dict[str, list[Record]]],
    config: Record,
) -> None:
    for group, records in sorted(groups.items()):
        file = files.add(
            _file_key(group),
            group.role,
            _fragments(layout, group, records, None),
            [_reference(config)] if group.role == "images" else [],
        )
        split = {
            table: rows
            for table, rows in records.items()
            if table in {"printing", "face_revision"} and group.partition == "bootstrap"
        }
        if split:
            detail = Group("text", "detail", group.kind, group.identifier, group.bucket)
            # Separate the positional detail from full-row detail fragments in this group.
            files.add(
                _file_key(detail) + "/columns",
                "text",
                _fragments(layout, detail, split, file),
                [_reference(file)],
            )


def export_snapshot(
    projection: Projection,
    ownership: Ownership,
    batch: Batch,
    *,
    brotli: Brotli | None = None,
) -> Snapshot:
    """Export fixed candidate 1.0.0 partitions and verify the independent join."""
    if not batch.data_version.startswith("preview-"):
        raise ValueError("Candidate exporter requires a preview data version")
    if set(projection.tables) != set(tables()):
        raise ValueError("Exact public collection whitelist required")
    properties = object_value(definition("Manifest")["properties"])
    profile = object_value(object_value(properties["partitioning"])["properties"])
    count = integer(object_value(profile["bucket_count"])["const"])
    if any(
        row["region"] not in batch.regions
        for table in ("printing", "face_revision", "product")
        for row in projection.tables[table]
    ):
        raise ValueError("Public region is outside the explicit manifest scope")
    layout = Layout(projection.tables, ownership, count)
    files = _Files(brotli)
    config = files.add("config", "config", projection.config, [])
    files.add("programs", "programs", {"format_version": "1.0.0", "entries": []}, [])
    _add_groups(layout, files, _partition(layout), config)
    contains: list[JsonValue] = [
        _reference(file)
        for key, file in sorted(files.files.items())
        if files.files[key]["role"] in {"bootstrap", "text", "config"}
    ]
    union = compress(
        canonical(
            {
                "format_version": "1.0.0",
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
        "format_version": "1.0.0",
        "data_version": batch.data_version,
        "published_at": batch.published_at,
        "regions": list(batch.regions),
        "languages": languages,
        "min_reader_version": "1.0.0",
        "required_capabilities": ["column-partition-v1", "fragment-container-v1"],
        "files": [files.files[key] for key in sorted(files.files)],
        "config_ref": _reference(config),
        "text_all": _description(union) | {"contains": contains},
        "partitioning": partitioning,
        "changes_ref": None,
    }
    validate("Manifest", manifest)
    result = Snapshot(manifest, files.payloads, union, recipe(brotli))
    result.verify(projection)
    return result
