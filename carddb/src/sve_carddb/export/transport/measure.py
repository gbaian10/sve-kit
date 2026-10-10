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
        "gzip": sum(len(blob.gzip) for blob in blobs),
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
    """Count metadata once, exclude text_all duplication, and expose failing gates."""
    if snapshot.compression_recipe["brotli"] != (
        None if brotli is None else brotli.version
    ):
        raise ValueError("Manifest measurement requires the same compressor recipe")
    manifest = compress(canonical(snapshot.manifest), brotli)
    files = {
        string(object_value(item)["key"]): object_value(item)
        for item in array(snapshot.manifest["files"])
    }
    text = [
        key
        for key in snapshot.payloads
        if files[key]["role"] in {"bootstrap", "text", "config"}
    ]
    initial = [
        key
        for key in snapshot.payloads
        if files[key]["role"] in {"bootstrap", "config"}
    ]
    shards: list[JsonValue] = []
    oversized: list[JsonValue] = []
    for key, blob in sorted(snapshot.payloads.items()):
        if files[key]["role"] in {"config", "programs"}:
            continue
        payload = object_value(parse(blob.raw))
        fragments = [
            object_value(item)
            for entries in object_value(payload["tables"]).values()
            for item in array(entries)
        ]
        set_buckets: list[JsonValue] = json_list(
            sorted({integer(f["bucket"]) for f in fragments})
        )
        set_partitions: list[JsonValue] = json_list(
            sorted({string(f["partition"]) for f in fragments})
        )
        shards.append(
            {
                "key": key,
                "role": files[key]["role"],
                "owners": [
                    parse(value)
                    for value in sorted({canonical(f["owner"]) for f in fragments})
                ],
                "buckets": set_buckets,
                "partitions": set_partitions,
                "rows": sum(len(array(f["rows"])) for f in fragments),
                **_sizes([blob]),
            }
        )
        if len(blob.raw) > SHARD_LIMIT:
            oversized.append(key)
    full = accounted_sizes(snapshot.payloads, text, manifest=manifest)
    bootstrap = accounted_sizes(snapshot.payloads, initial, manifest=manifest)
    images = [
        blob
        for key, blob in snapshot.payloads.items()
        if files[key]["role"] == "images"
    ]
    startup: dict[str, JsonValue] = {
        string(region): bootstrap
        | {
            "file_count": len(initial) + 1,
            "scope": "all bootstrap and dependency closure; shared/mixed files counted in full",
            "target_br_1_mib": bootstrap["br"] is not None
            and integer(bootstrap["br"]) <= MIB,
            "stop_for_maintainer": bootstrap["br"] is not None
            and integer(bootstrap["br"]) > 2 * MIB,
        }
        for region in array(snapshot.manifest["regions"])
    }
    return {
        "text_total": full,
        "bootstrap": bootstrap,
        "startup_by_region": startup,
        "images_metadata": {"file_count": len(images), **_sizes(images)},
        "manifest": _sizes([manifest]),
        "text_all": _sizes([snapshot.text_all]),
        "shard_count": len(shards),
        "shards": shards,
        "oversized_shards": oversized,
        "gates": {
            "raw_40_mib": integer(full["raw"]) <= 40 * MIB,
            "br_8_mib": full["br"] is not None and integer(full["br"]) <= 8 * MIB,
            "gzip_10_mib": integer(full["gzip"]) <= 10 * MIB,
            "shards_512_kib": not oversized,
        },
    }


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
