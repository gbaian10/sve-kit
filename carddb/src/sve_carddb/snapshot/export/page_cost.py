"""Metadata-only page replay; persisted bytes and decoded rows are distinct caches."""

from collections import OrderedDict
from dataclasses import dataclass, field
from math import ceil
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import array, object_value, parse, string

if TYPE_CHECKING:
    from sve_carddb.snapshot.export import Snapshot

PAGE_SIZE = 24
CACHE_BYTES = 12 * 1024 * 1024
CACHE_FILES = 64


def _percentiles(values: list[int]) -> dict[str, JsonValue]:
    ordered = sorted(values)
    return {
        "p50": ordered[max(0, ceil(len(values) * 0.5) - 1)] if values else 0,
        "p95": ordered[max(0, ceil(len(values) * 0.95) - 1)] if values else 0,
        "max": max(values, default=0),
    }


def _index(
    snapshot: Snapshot,
) -> tuple[dict[str, list[tuple[str, str]]], list[list[str]]]:
    bindings: dict[str, list[tuple[str, str]]] = {}
    owners: dict[str, set[str]] = {}
    regions: dict[str, str] = {}
    for key, blob in snapshot.payloads.items():
        value = object_value(parse(blob.raw))
        for table, entries in object_value(value.get("tables", {})).items():
            if table not in {"printing", "printing_image"}:
                continue
            for raw in array(entries):
                fragment = object_value(raw)
                columns = list(map(string, array(fragment["columns"])))
                for raw_row in array(fragment["rows"]):
                    row = dict(zip(columns, array(raw_row), strict=True))
                    if table == "printing" and fragment["partition"] == "bootstrap":
                        regions[string(row["id"])] = string(row["region"])
                    elif table == "printing_image":
                        printing = string(row["printing_id"])
                        bindings.setdefault(printing, []).append(
                            (key, string(row["image_id"]))
                        )
                        owner = string(object_value(fragment["owner"])["id"])
                        owners.setdefault(owner, set()).add(printing)
    return bindings, _pages(owners, regions)


def _pages(owners: dict[str, set[str]], regions: dict[str, str]) -> list[list[str]]:
    grouped: dict[tuple[str, str], set[str]] = {}
    for owner, printings in owners.items():
        for printing in printings:
            grouped.setdefault((regions[printing], owner), set()).add(printing)
    return [
        sorted(printings)[start : start + PAGE_SIZE]
        for _, printings in sorted(grouped.items())
        for start in range(0, len(printings), PAGE_SIZE)
    ]


@dataclass
class Replay:
    cache: OrderedDict[str, int] = field(default_factory=OrderedDict)
    persisted: set[str] = field(default_factory=set)
    counts: list[int] = field(default_factory=list)
    raw: list[int] = field(default_factory=list)
    br: list[int] = field(default_factory=list)
    gzip: list[int] = field(default_factory=list)
    images: list[int] = field(default_factory=list)
    evictions: int = 0
    hits: int = 0
    max_cache: int = 0
    max_files: int = 0

    def page(self, snapshot: Snapshot, needed: set[str], wanted: set[str]) -> None:
        """Pin required metadata and evict only unpinned bytes, never page rows."""
        self.counts.append(len(needed))
        self.images.append(len(wanted))
        self.raw.append(sum(len(snapshot.payloads[key].raw) for key in needed))
        self.br.append(sum(len(snapshot.payloads[key].br or b"") for key in needed))
        self.gzip.append(sum(len(snapshot.payloads[key].gzip) for key in needed))
        self.persisted |= needed
        for key in sorted(needed):
            self.hits += int(key in self.cache)
            self.cache[key] = len(snapshot.payloads[key].raw)
            self.cache.move_to_end(key)
        for key in list(self.cache):
            if (
                sum(self.cache.values()) <= CACHE_BYTES
                and len(self.cache) <= CACHE_FILES
            ):
                break
            if key not in needed:
                del self.cache[key]
                self.evictions += 1
        self.max_cache = max(self.max_cache, sum(self.cache.values()))
        self.max_files = max(self.max_files, len(self.cache))


def page_image_cost(snapshot: Snapshot) -> dict[str, JsonValue]:
    """Replay sorted 24-printing owner pages with all their face images, without blobs."""
    bindings, pages = _index(snapshot)
    replay = Replay()
    has_br = all(blob.br is not None for blob in snapshot.payloads.values())
    for page in pages:
        wanted = {image for printing in page for _, image in bindings[printing]}
        needed = {key for printing in page for key, _ in bindings[printing]}
        replay.page(snapshot, needed, wanted)
    return {
        "model": "sorted owner/region pages; 24 printings, all bound images; metadata only",
        "source_details_required": False,
        "page_count": len(pages),
        "images_per_page": _percentiles(replay.images),
        "cold": {
            "requests": _percentiles(replay.counts),
            "raw": _percentiles(replay.raw),
            "br": _percentiles(replay.br) if has_br else None,
            "gzip": _percentiles(replay.gzip),
        },
        "warm": {"requests": 0, "raw": 0, "br": 0 if has_br else None, "gzip": 0},
        "warm_assumption": "same page after successful persisted byte cache, same manifest; no blobs",
        "session_unique_metadata": {
            "files": len(replay.persisted),
            "raw": sum(len(snapshot.payloads[k].raw) for k in replay.persisted),
            "br": sum(len(snapshot.payloads[k].br or b"") for k in replay.persisted)
            if has_br
            else None,
            "gzip": sum(len(snapshot.payloads[k].gzip) for k in replay.persisted),
        },
        "lru": {
            "byte_limit": CACHE_BYTES,
            "file_limit": CACHE_FILES,
            "max_raw_footprint": replay.max_cache,
            "max_files": replay.max_files,
            "max_page_pin_raw": max(replay.raw, default=0),
            "hits": replay.hits,
            "evictions": replay.evictions,
            "fits": replay.max_cache <= CACHE_BYTES and replay.max_files <= CACHE_FILES,
        },
        "decoded_rows": "retain only current page image rows; browser heap is not measured here",
        "image_blob_bytes": None,
    }
