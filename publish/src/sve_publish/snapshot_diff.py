"""Trust the two committed manifests instead of fetching every immutable payload."""

from typing import TYPE_CHECKING

from jsonschema import ValidationError as SchemaError
from sve_carddb.export.read_api import (
    ExportError,
    array,
    integer,
    object_value,
    retained_manifest,
    string,
)

if TYPE_CHECKING:
    from pydantic import JsonValue
    from sve_carddb.export.read_api import Member

    from sve_publish.adapter import R2Store


def missing_members(
    store: R2Store, index: dict[str, JsonValue] | None, members: tuple[Member, ...]
) -> tuple[Member, ...]:
    """Only explicitly declared representations count as previously uploaded."""
    known: dict[str, int] = {}
    if index is not None:
        try:
            for name in ("current", "previous"):
                if index[name] is not None:
                    _retain(store, object_value(index[name]), known)
        except ExportError:
            raise
        except ValueError, TypeError, KeyError, SchemaError:
            raise ExportError("Retained snapshot manifest is invalid") from None
    for member in members:
        if member.key in known and known[member.key] != len(member.raw):
            raise ExportError(f"Retained representation length conflicts: {member.key}")
    return tuple(member for member in members if member.key not in known)


def _retain(store: R2Store, entry: dict[str, JsonValue], known: dict[str, int]) -> None:
    path = string(entry["manifest_path"])
    if path != "snapshots/manifests/" + string(entry["manifest_sha256"])[7:] + ".json":
        raise ExportError("Retained manifest path differs from its hash")
    remote = store.get(path)
    if remote is None:
        raise ExportError("Snapshot upload requires intact retained manifests")
    manifest = retained_manifest(remote.raw, entry)
    # Manifest .br has no descriptor, so only its raw representation is known.
    _remember(known, path, len(remote.raw))
    rows = [object_value(row) for row in array(manifest["files"])]
    if manifest["changes_ref"] is not None:
        rows.append(object_value(manifest["changes_ref"]))
    for row in rows:
        key = string(row["path"])
        _remember(known, key, integer(row["bytes"]))
        compressed = object_value(row["compressed_bytes"])["br"]
        if compressed is not None:
            _remember(known, key + ".br", integer(compressed))


def _remember(known: dict[str, int], key: str, length: int) -> None:
    if key in known and known[key] != length:
        raise ExportError(f"Retained representation length conflicts: {key}")
    known[key] = length
