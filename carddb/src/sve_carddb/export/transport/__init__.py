"""Export the public logical projection to immutable candidate transport blobs."""

from concurrent.futures import Future, ThreadPoolExecutor
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.profiles import MEDIA
from sve_carddb.contracts.profiles import profile as profile_for
from sve_carddb.contracts.snapshot import columns, definition, row_type, tables
from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.export.name_annotations import compact_names, whole_name
from sve_carddb.export.project.source import json_list
from sve_carddb.export.reader import join_payloads, read_snapshot, validate_manifest
from sve_carddb.export.semantics import row_key
from sve_carddb.export.transport.compression import Blob, Brotli, compress, recipe
from sve_carddb.export.transport.family import FamilyLayout, Scope
from sve_carddb.export.transport.layout import Layout, Ownership
from sve_carddb.export.transport.wire import container, encode

if TYPE_CHECKING:
    from sve_carddb.export.project import Projection

type Record = dict[str, JsonValue]

__all__ = [
    "Batch",
    "Blob",
    "Brotli",
    "Ownership",
    "Snapshot",
    "columns",
    "container",
    "encode",
    "export_snapshot",
    "row_key",
    "row_type",
    "whole_name",
]


@dataclass(frozen=True)
class Batch:
    """Candidate batch metadata is kept outside reusable payload bytes."""

    data_version: str
    published_at: str
    data_as_of: str
    regions: tuple[str, ...]


@dataclass(frozen=True)
class Snapshot:
    """Verified exporter candidate; the preview writer reuses its sealed bytes."""

    manifest: Record
    payloads: dict[str, Blob]
    compression_recipe: dict[str, str | int | None]
    manifest_blob: Blob
    source_files: dict[str, frozenset[str]] = field(default_factory=dict)

    def verify(self, projection: Projection) -> None:
        """Require the reader's join to equal the logical view."""
        joined = read_snapshot(
            self.manifest, {key: blob.raw for key, blob in self.payloads.items()}
        )
        if joined != projection.tables:
            raise ValueError("Export join differs from public logical projection")

    def assert_identical(self, other: Snapshot) -> None:
        """Compare exact raw and compressed payload bytes."""
        if (
            self.payloads != other.payloads
            or self.compression_recipe != other.compression_recipe
        ):
            raise ValueError("Clean exports differ in payload or compression bytes")


def _reference(file: Record) -> Record:
    return {"key": file["key"], "sha256": file["sha256"]}


def _description(raw: bytes) -> Record:
    hashed = digest(raw)
    return {
        "path": "snapshots/blobs/" + hashed[7:] + ".json",
        "sha256": hashed,
        "bytes": len(raw),
    }


def _compressed_bytes(blob: Blob) -> Record:
    return {
        "br": None if blob.br is None else len(blob.br),
    }


class _Files:
    def __init__(self, brotli: Brotli | None, pool: ThreadPoolExecutor) -> None:
        self.brotli = brotli
        self.pool = pool
        self.pending: dict[str, Future[Blob]] = {}
        self.files: dict[str, Record] = {}
        self.raw: dict[str, bytes] = {}

    def add(
        self,
        key: str,
        role: str,
        value: Record,
        dependencies: list[JsonValue],
        *,
        scope: Scope | None = None,
    ) -> Record:
        """Seal raw bytes so dependent shards can bind their base hash."""
        if key in self.files:
            raise ValueError("Duplicate logical file key")
        raw = canonical(value)
        purpose = scope.purpose if scope is not None else role
        limit = 2 * 1024 * 1024 if purpose == "catalog" else 512 * 1024
        if role not in {"config", "programs"} and len(raw) > limit:
            raise ValueError(
                f"Snapshot shard exceeds raw limit: {key} ({len(raw)} > {limit})"
            )
        counts: list[JsonValue] = [
            [table, array(f[0])[0], array(f[0])[1], f[1], f[2], len(array(f[4]))]
            for table, fragments in object_value(value.get("tables", {})).items()
            for fragment in array(fragments)
            for f in (array(fragment),)
        ]
        file = _description(raw) | {
            "key": key,
            "role": role,
            "purpose": purpose,
            "region": (scope.region or None) if scope is not None else None,
            "segment": scope.segment if scope is not None else "shared",
            "row_counts": counts,
            "dependencies": dependencies,
        }
        self.files[key] = file
        self.raw[key] = raw
        self.pending[key] = self.pool.submit(compress, raw, self.brotli)
        return file

    def snapshot(
        self,
        manifest: Record,
        projection: Projection,
        source_files: dict[str, frozenset[str]],
    ) -> Snapshot:
        """Join sealed raw bytes while compression fills only transfer lengths."""
        joined = join_payloads(manifest, self.raw)
        if joined != projection.tables:
            raise ValueError("Export join differs from public logical projection")
        payloads = {key: future.result() for key, future in self.pending.items()}
        complete = manifest | {
            "files": [
                object_value(file)
                | {
                    "compressed_bytes": _compressed_bytes(
                        payloads[string(object_value(file)["key"])]
                    )
                }
                for file in array(manifest["files"])
            ],
        }
        validate_manifest(complete)
        return Snapshot(
            complete,
            payloads,
            recipe(self.brotli),
            compress(canonical(complete), self.brotli),
            source_files,
        )


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
    with ThreadPoolExecutor(max_workers=2) as shards:
        files = _Files(brotli, shards)
        config = files.add(
            "config",
            "config",
            projection.config | {"format_version": format_version},
            [],
        )
        files.add(
            "programs",
            "programs",
            {"format_version": format_version, "entries": []},
            [],
        )
        source_files = FamilyLayout(layout).seal(files, config)
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
            "data_as_of": batch.data_as_of,
            "regions": list(batch.regions),
            "languages": languages,
            "min_reader_version": format_version,
            "required_capabilities": list(selected.capabilities),
            "files": [files.files[key] for key in sorted(files.files)],
            "config_ref": _reference(config),
            "partitioning": partitioning,
            "changes_ref": None,
        }
        return files.snapshot(manifest, projection, source_files)
