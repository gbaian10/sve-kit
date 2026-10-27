"""Numeric candidate capacity and immutable-file update measurements."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, integer, object_value, parse, string
from sve_carddb.export.project.source import json_list
from sve_carddb.export.transport.compression import Blob, Brotli, compress

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from sve_carddb.export.transport import Snapshot

MIB = 1048576
SHARD_LIMIT = 512 * 1024


def _sizes(blobs: list[Blob]) -> dict[str, JsonValue]:
    return {
        "raw": sum(len(blob.raw) for blob in blobs),
        "br": None
        if any(blob.br is None for blob in blobs)
        else sum(len(blob.br or b"") for blob in blobs),
    }


def accounted_sizes(
    payloads: Mapping[str, Blob], keys: Iterable[str], *, manifest: Blob | None = None
) -> dict[str, JsonValue]:
    """Shared and mixed files contribute their whole bytes once per closure."""
    blobs = [payloads[key] for key in sorted(set(keys))]
    return _sizes(blobs if manifest is None else [manifest, *blobs])


def measure(
    snapshot: Snapshot, *, brotli: Brotli | None = None
) -> dict[str, JsonValue]:
    """Count metadata once and expose failing gates."""
    if snapshot.compression_recipe["brotli"] != (
        None if brotli is None else brotli.version
    ):
        raise ValueError("Manifest measurement requires the same compressor recipe")
    manifest = snapshot.manifest_blob
    files = {
        string(object_value(item)["key"]): object_value(item)
        for item in array(snapshot.manifest["files"])
    }

    def closure(keys: set[str]) -> set[str]:
        pending = list(keys)
        while pending:
            key = pending.pop()
            for raw in array(files[key]["dependencies"]):
                target = string(object_value(raw)["key"])
                if target not in keys:
                    keys.add(target)
                    pending.append(target)
        return keys

    text = {
        key
        for key, file in files.items()
        if file["role"] in {"bootstrap", "text", "config"}
    }
    startup: dict[str, JsonValue] = {}
    complete: dict[str, JsonValue] = {}
    for raw_region in array(snapshot.manifest["regions"]):
        region = string(raw_region)
        initial = closure(
            {
                key
                for key, file in files.items()
                if file["purpose"] == "config"
                or (file["purpose"] == "catalog" and file["region"] in {region, None})
            }
        )
        sizes = accounted_sizes(snapshot.payloads, initial, manifest=manifest)
        startup[region] = sizes | {
            "file_count": len(initial) + 1,
            "keys": json_list(sorted(initial)),
            "scope": "selected and shared catalog plus strong dependencies",
            "target_br_1_mib": sizes["br"] is not None and integer(sizes["br"]) <= MIB,
            "stop_for_maintainer": sizes["br"] is not None
            and integer(sizes["br"]) > 2 * MIB,
        }
        c0 = closure({key for key in text if files[key]["region"] in {region, None}})
        sources = {
            string(object_value(ref)["key"])
            for key in c0
            for ref in array(files[key]["source_refs"])
        }
        full = closure(c0 | sources)
        complete[region] = accounted_sizes(
            snapshot.payloads, full, manifest=manifest
        ) | {"file_count": len(full) + 1, "keys": json_list(sorted(full))}
    shards, oversized = _shards(snapshot, files)
    images = [
        blob
        for key, blob in snapshot.payloads.items()
        if files[key]["role"] == "images"
    ]
    return {
        "text_total": accounted_sizes(snapshot.payloads, text, manifest=manifest),
        "bootstrap": accounted_sizes(
            snapshot.payloads,
            {
                key
                for key, file in files.items()
                if file["purpose"] in {"catalog", "config"}
            },
            manifest=manifest,
        ),
        "startup_by_region": startup,
        "complete_text_by_region": complete,
        "images_metadata": {"file_count": len(images), **_sizes(images)},
        "manifest": _sizes([manifest]),
        "shard_count": len(shards),
        "shards": shards,
        "oversized_shards": oversized,
        "gates": {
            "br_10_mib": all(
                object_value(v)["br"] is not None
                and integer(object_value(v)["br"]) <= 10 * MIB
                for v in complete.values()
            ),
            "shards_512_kib": not oversized,
        },
    }


def _shards(
    snapshot: Snapshot, files: dict[str, dict[str, JsonValue]]
) -> tuple[list[JsonValue], list[JsonValue]]:
    shards: list[JsonValue] = []
    oversized: list[JsonValue] = []
    for key, blob in sorted(snapshot.payloads.items()):
        file = files[key]
        if file["role"] in {"config", "programs"}:
            continue
        counts = [array(c) for c in array(file["row_counts"])]
        limit = 2 * MIB if file["purpose"] == "catalog" else SHARD_LIMIT
        shards.append(
            {
                "key": key,
                "role": file["role"],
                "purpose": file["purpose"],
                "region": file["region"],
                "segment": file["segment"],
                "owners": [
                    parse(v) for v in sorted({canonical([c[1], c[2]]) for c in counts})
                ],
                "partitions": json_list(sorted({string(c[3]) for c in counts})),
                "rows": sum(integer(c[5]) for c in counts),
                "limit_raw": limit,
                **_sizes([blob]),
            }
        )
        if len(blob.raw) > limit:
            oversized.append(key)
    return shards, oversized


def update(
    previous: Snapshot, current: Snapshot, *, brotli: Brotli | None = None
) -> dict[str, JsonValue]:
    """Measure replacement files, including positional details rebuilt by base hashes."""
    changed = [
        key
        for key, blob in sorted(current.payloads.items())
        if key not in previous.payloads or blob.raw != previous.payloads[key].raw
    ]
    return {
        "changed_keys": list(changed),
        "changed_file_count": len(changed),
        "transfer": _sizes(
            [
                compress(canonical(current.manifest), brotli),
                *(current.payloads[key] for key in changed),
            ]
        ),
    }
