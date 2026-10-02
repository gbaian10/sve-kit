"""Write size-bounded YAML candidates and prose-free proofs outside immutable inputs."""

import shutil
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.registry.storage import MAX_BYTES, TARGET_BYTES, encode
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_sources.models import Inventory

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue

    from sve_carddb.template_sources.inventory import Scan
    from sve_carddb.template_sources.models import Entry, Recipe

CHUNK_ENTRIES = 256


def _chunks(
    pins: tuple[Recipe, ...], entries: tuple[Entry, ...]
) -> Iterator[tuple[bytes, str]]:
    inventory = Inventory(recipes=pins, entries=entries)
    raw = encode(inventory)
    if len(raw) > TARGET_BYTES and len(entries) > 1:
        midpoint = len(entries) // 2
        yield from _chunks(pins, entries[:midpoint])
        yield from _chunks(pins, entries[midpoint:])
    elif len(raw) >= MAX_BYTES:
        raise ValueError("Template inventory envelope exceeds the authored size limit")
    else:
        yield raw, digest(canonical(inventory.model_dump(mode="json")))


def write(
    output: Path, scan: Scan, report: dict[str, JsonValue], *, inputs: tuple[Path, ...]
) -> None:
    """Publish a fresh result directory atomically; never overwrite prior evidence."""
    target = output.resolve()
    if (
        output.is_symlink()
        or output.exists()
        or any(target.is_relative_to(path.resolve()) for path in inputs)
    ):
        raise ValueError("Template output must be new and outside all immutable inputs")
    output.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".template-checkpoint-", dir=output.parent))
    try:
        inventories: dict[str, JsonValue] = {}
        destination = staging / "template-sources"
        destination.mkdir()
        sorted_entries = tuple(sorted(scan.entries, key=lambda item: item.id))
        sequence = 0
        for start in range(0, max(1, len(sorted_entries)), CHUNK_ENTRIES):
            for raw, checksum in _chunks(
                scan.recipes, sorted_entries[start : start + CHUNK_ENTRIES]
            ):
                sequence += 1
                path = f"template-sources/{sequence:03d}.yaml"
                (staging / path).write_bytes(raw)
                inventories[path] = checksum
        report["inventories"] = inventories
        (staging / "checkpoint.json").write_bytes(canonical(report) + b"\n")
        with (staging / "source-coverage.jsonl").open("wb") as stream:
            for proof in (*scan.pages, *scan.fields):
                stream.write(canonical(proof) + b"\n")
        staging.rename(output)
    finally:
        if staging.exists():
            shutil.rmtree(staging)
