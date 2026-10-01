"""Numeric candidate capacity and immutable-file update measurements."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.export.compression import Blob, Brotli, compress
from sve_carddb.snapshot.values import (
    array,
    canonical,
    integer,
    object_value,
    parse,
    string,
)

if TYPE_CHECKING:
    from sve_carddb.snapshot.export import Snapshot

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
        blob
        for key, blob in snapshot.payloads.items()
        if files[key]["role"] in {"bootstrap", "text", "config"}
    ]
    initial = [
        blob
        for key, blob in snapshot.payloads.items()
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
        shards.append(
            {
                "key": key,
                "role": files[key]["role"],
                "owners": [
                    parse(value)
                    for value in sorted({canonical(f["owner"]) for f in fragments})
                ],
                "bucket": fragments[0]["bucket"],
                "partition": fragments[0]["partition"],
                "rows": sum(len(array(f["rows"])) for f in fragments),
                **_sizes([blob]),
            }
        )
        if len(blob.raw) > SHARD_LIMIT:
            oversized.append(key)
    full = _sizes([manifest, *text])
    bootstrap = _sizes([manifest, *initial])
    return {
        "text_total": full,
        "bootstrap": bootstrap,
        "manifest": _sizes([manifest]),
        "text_all": _sizes([snapshot.text_all]),
        "shard_count": len(shards),
        "shards": shards,
        "oversized_shards": oversized,
        "gates": {
            "raw_40_mib": integer(full["raw"]) <= 40 * MIB,
            "br_8_mib": full["br"] is not None and integer(full["br"]) <= 8 * MIB,
            "gzip_10_mib": integer(full["gzip"]) <= 10 * MIB,
            "bootstrap_br_1_mib": bootstrap["br"] is not None
            and integer(bootstrap["br"]) <= MIB,
            "bootstrap_gzip_1_mib": integer(bootstrap["gzip"]) <= MIB,
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
