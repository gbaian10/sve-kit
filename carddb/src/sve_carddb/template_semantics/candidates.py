"""Private candidate envelopes with an explicitly supplied index hash and complete shard closure."""

import os
import shutil
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_translations.files import json_bytes
from sve_carddb.template_translations.replay_models import InventoryV2

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue

    from sve_carddb.template_sources.models import Recipe
    from sve_carddb.template_translations.replay_models import ReplayContext
    from sve_carddb.template_translations.sources import SourceReplay


def chunks(
    pins: tuple[Recipe, ...], context: ReplayContext, replay: SourceReplay
) -> Iterator[bytes]:
    """One historical group is split only for the authored envelope byte limit."""
    entries = tuple(m.entry for m in sorted(replay.entries, key=lambda m: m.entry.id))
    pending = [entries]
    while pending:
        part = pending.pop(0)
        inventory = InventoryV2(
            template_source_format=2,
            kind="template_source_inventory",
            recipes=pins,
            replay_context=context,
            entries=part,
        )
        raw = canonical(inventory.model_dump(mode="json"))
        if len(raw) >= 768 * 1024 and len(part) > 1:
            middle = len(part) // 2
            pending[0:0] = [part[:middle], part[middle:]]
        elif len(raw) >= 1024 * 1024:
            raise ValueError("Replay inventory envelope exceeds authored size limit")
        else:
            yield raw


def write(
    destination: Path,
    pins: tuple[Recipe, ...],
    context: ReplayContext,
    replay: SourceReplay,
    *,
    inputs: tuple[Path, ...],
) -> str:
    """Publish fresh private candidates; never write authored or declare adoption."""
    if (
        destination.exists()
        or destination.is_symlink()
        or any(destination.resolve().is_relative_to(p.resolve()) for p in inputs)
    ):
        raise ValueError("Replay candidate output must be fresh and outside inputs")
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(
        tempfile.mkdtemp(prefix=".replay-candidate-", dir=destination.parent)
    )
    try:
        records: list[JsonValue] = []
        for ordinal, raw in enumerate(chunks(pins, context, replay), start=1):
            name = f"{ordinal:03}.yaml"
            (staging / name).write_bytes(raw)
            records.append({"path": name, "hash": digest(raw)})
        content = canonical(
            {"format": 1, "kind": "template_replay_candidate", "files": records}
        )
        (staging / "index.json").write_bytes(content)
        for path in staging.iterdir():
            path.chmod(0o600)
            with path.open("rb") as stream:
                os.fsync(stream.fileno())
        staging.rename(destination)
        return digest(content)
    finally:
        if staging.exists():
            shutil.rmtree(staging)


def read(root: Path, expected_hash: str) -> tuple[InventoryV2, ...]:
    """Separately supplied evidence prevents a consumer from trusting a mutable self-filled baseline."""
    index = root / "index.json"
    if root.is_symlink() or index.is_symlink() or not index.is_file():
        raise ValueError("Replay candidate inputs must be regular files")
    raw = index.read_bytes()
    if digest(raw) != expected_hash:
        raise ValueError("Replay candidate index exact hash mismatch")
    data = object_value(parse(raw))
    if (
        set(data) != {"format", "kind", "files"}
        or type(data["format"]) is not int
        or data["format"] != 1
        or data["kind"] != "template_replay_candidate"
    ):
        raise ValueError("Replay candidate index requires its closed format")
    records = [object_value(r) for r in array(data["files"])]
    names = [str(row.get("path")) for row in records]
    if (
        not records
        or names != [f"{n:03}.yaml" for n in range(1, len(records) + 1)]
        or {p.name for p in root.iterdir()} != {"index.json", *names}
    ):
        raise ValueError("Replay candidate shard closure mismatch")
    result = []
    for record, name in zip(records, names, strict=True):
        path = root / name
        if set(record) != {"path", "hash"} or path.is_symlink() or not path.is_file():
            raise ValueError("Replay candidate inputs must be regular files")
        content = path.read_bytes()
        if digest(content) != record["hash"]:
            raise ValueError("Replay candidate shard exact hash mismatch")
        result.append(InventoryV2.model_validate_json(json_bytes(content)))
    return tuple(result)
