"""Explicit frozen formal release bundle; never promote a preview or infer a v."""

import os
from dataclasses import asdict
from pathlib import Path
from typing import TYPE_CHECKING

from jsonschema import ValidationError as SchemaError
from pydantic import JsonValue

from sve_carddb.r2_upload.plan import read_member
from sve_carddb.snapshot.export import Blob, Snapshot
from sve_carddb.snapshot.media import MediaPlan
from sve_carddb.snapshot.project import Projection
from sve_carddb.snapshot.publish.plan import JSON_KEY, prepare, verify_media
from sve_carddb.snapshot.publish.state import Checkpoint, Ledger, atomic, attempt
from sve_carddb.snapshot.publish.storage import PublishError
from sve_carddb.snapshot.reader import read_snapshot
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)

if TYPE_CHECKING:
    from sve_carddb.snapshot.export import Brotli
    from sve_carddb.snapshot.publish.plan import Release


_FIELDS = {
    "format",
    "manifest_path",
    "media_state",
    "assets",
    "confirmed_images",
    "compression_recipe",
}


def directory(root: Path) -> None:
    """Keep explicit input and evidence paths from following symlink aliases."""
    if not root.is_absolute() or root.resolve() != root or not root.is_dir():
        raise PublishError("Explicit existing non-symlink directory required")


def ledger_at(root: Path, backup: Path) -> Ledger:
    """CLI refuses automatic initialization or recovery, including in dry-run."""
    directory(root)
    directory(backup)
    ledger = Ledger(root, backup)
    ledger.verify_backup()
    return ledger


def checkpoint_path(path: Path, ledger: Ledger) -> None:
    """A checkpoint beside the same rollback-prone copy cannot prove an upper bound."""
    directory(path.parent)
    if path.resolve() != path or any(
        path.is_relative_to(root) for root in (ledger.root, ledger.backup)
    ):
        raise PublishError("Checkpoint must be outside both ledger roots")
    if path.exists() and (not path.is_file() or path.stat().st_mode & 0o077):
        raise PublishError("Checkpoint requires a private regular file")


def verify_checkpoint(path: Path, ledger: Ledger) -> None:
    """Require the previously independently saved checkpoint, never create a guess."""
    checkpoint_path(path, ledger)
    value = object_value(parse(path.read_bytes()))
    expected = ledger.checkpoint()
    if value != asdict(expected):
        raise PublishError("Independent checkpoint differs from the verified ledger")


def save_checkpoint(path: Path, ledger: Ledger) -> None:
    """Advance the independent pin after successful or failed but durable writes."""
    checkpoint_path(path, ledger)
    with ledger.exclusive():
        ledger.verify_backup()
        proof = Checkpoint(
            integer(ledger.read()["high_water"]),
            digest(ledger.path.read_bytes()),
            digest(ledger.receipts.read_bytes()),
        )
        previous = os.umask(0o077)
        try:
            atomic(path, canonical(asdict(proof)))
        finally:
            os.umask(previous)


def _blob(root: Path, row: dict[str, JsonValue]) -> Blob:
    path = string(row["path"])
    if not JSON_KEY.fullmatch(path) or not path.endswith(".json"):
        raise PublishError("Invalid frozen JSON member path")
    encodings = object_value(row["compressed_bytes"])
    return Blob(
        read_member(root, path),
        None if encodings["br"] is None else read_member(root, path + ".br"),
        read_member(root, path + ".gz"),
    )


def _inventory(root: Path, allowed: set[str]) -> None:
    found = {"release.json"}
    if {p.name for p in root.iterdir()} - {"release.json", "snapshots", "sources"}:
        raise PublishError("Unexpected frozen bundle root member")
    for name in ("snapshots", "sources"):
        subtree = root / name
        if not subtree.exists() and not subtree.is_symlink():
            continue
        if subtree.is_symlink() or not subtree.is_dir():
            raise PublishError("Frozen bundle subtree must be a directory")
        for parent, dirs, files in os.walk(subtree, followlinks=False):
            if any((Path(parent) / d).is_symlink() for d in dirs):
                raise PublishError("Frozen bundle contains a symlink directory")
            for f in files:
                key = (Path(parent) / f).relative_to(root).as_posix()
                read_member(root, key)
                found.add(key)
    if found != allowed | {"release.json"}:
        raise PublishError("Frozen bundle inventory differs from sealed members")


def load_bundle(
    root: Path, ledger: Ledger, *, cdn_root: str, brotli: Brotli | None = None
) -> Release:
    """Validate the full transport and reservation before any credential/network access."""
    try:
        return _load(root, ledger, cdn_root=cdn_root, brotli=brotli)
    except PublishError:
        raise
    except ValueError, OSError, KeyError, TypeError, SchemaError:
        raise PublishError("Frozen formal release bundle validation failed") from None


def _load(
    root: Path, ledger: Ledger, *, cdn_root: str, brotli: Brotli | None
) -> Release:
    directory(root)
    private = object_value(parse(read_member(root, "release.json")))
    if (
        set(private) != _FIELDS
        or type(private["format"]) is not int
        or private["format"] != 1
    ):
        raise PublishError("Invalid frozen release descriptor")
    path = string(private["manifest_path"])
    if (
        not JSON_KEY.fullmatch(path)
        or not path.startswith("snapshots/manifests/")
        or not path.endswith(".json")
    ):
        raise PublishError("Invalid frozen manifest path")
    manifest_raw = read_member(root, path)
    if digest(manifest_raw)[7:] != Path(path).stem:
        raise PublishError("Frozen manifest content address mismatch")
    manifest = object_value(parse(manifest_raw))
    payloads = {
        string(row["key"]): _blob(root, row)
        for raw in array(manifest["files"])
        for row in (object_value(raw),)
    }
    config = object_value(parse(payloads["config"].raw))
    config.pop("format_version")
    view = Projection(
        read_snapshot(manifest, {k: b.raw for k, b in payloads.items()}), config, {}
    )
    recipe = object_value(private["compression_recipe"])
    if any(
        value is not None and not isinstance(value, (str, int))
        for value in recipe.values()
    ):
        raise PublishError("Invalid frozen compression recipe")
    from typing import cast  # ruff: ignore[import-outside-top-level] -- validated JSON recipe cannot retain mutable arrays or objects

    snapshot = Snapshot(
        manifest,
        payloads,
        _blob(root, object_value(manifest["text_all"])),
        cast("dict[str, str | int | None]", recipe),
    )
    media = MediaPlan(
        view,
        object_value(private["media_state"]),
        tuple(object_value(a) for a in array(private["assets"])),
    )
    for asset in media.assets:
        read_member(root / "sources", string(asset["source"]))
    release = prepare(
        snapshot,
        media,
        root / "sources",
        cdn_root=cdn_root,
        brotli=brotli,
        changes=None
        if manifest["changes_ref"] is None
        else read_member(root, string(object_value(manifest["changes_ref"])["path"])),
        confirmed_images=frozenset(
            string(v) for v in array(private["confirmed_images"])
        ),
    )
    state = ledger.read()
    reservation = attempt(state, integer(media.state["revision"]))
    if (
        reservation["data_version"] != manifest["data_version"]
        or reservation["status"] == "superseded"
    ):
        raise PublishError("Frozen release differs from its durable reservation")
    if reservation["plan"] is None:
        verify_media(release, ledger.media_basis())
    elif any(
        object_value(reservation["plan"])[k] != v for k, v in release.describe().items()
    ):
        raise PublishError("Frozen retry differs from its sealed plan")
    _inventory(
        root,
        {m.key for m in release.members}
        | {"sources/" + string(a["source"]) for a in release.assets},
    )
    return release


def write_bundle(root: Path, release: Release, *, brotli: Brotli | None = None) -> None:
    """Upstream formal-gate callers freeze their already-reserved release explicitly."""
    directory(root)
    if (
        prepare(
            release.snapshot,
            release.media,
            release.source,
            cdn_root=release.cdn_root,
            brotli=brotli,
            changes=None if release.changes is None else canonical(release.changes),
            confirmed_images=release.confirmed_images,
        )
        != release
    ):
        raise PublishError("Frozen bundle requires an unchanged validated release")
    if any(root.iterdir()):
        raise PublishError("Frozen bundle destination must be empty")
    for member in release.members:
        path = root / member.key
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(member.raw)
    assets = {string(a["path"]): a for a in release.assets}
    for key, raw in release.images():
        path = root / "sources" / string(assets[key]["source"])
        if not path.is_relative_to(root) or ".." in path.parts:
            raise PublishError("Frozen source path is outside the bundle")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
    (root / "release.json").write_bytes(
        canonical(
            {
                "format": 1,
                "manifest_path": release.entry["manifest_path"],
                "media_state": release.media.state,
                "assets": list(release.media.assets),
                "confirmed_images": list(release.confirmed_images),
                "compression_recipe": dict(release.snapshot.compression_recipe),
            }
        )
    )


def report(release: Release, ledger: Ledger) -> dict[str, object]:
    """Counts are upload candidates, not claims about a remote inventory."""
    versions = [
        integer(r[field])
        for r in release.media.projection.tables["printing_image"]
        for field in ("card_version", "art_version")
        if r[field] is not None
    ]
    state = ledger.read()
    current = None
    for raw in array(state["attempts"]):
        item = object_value(raw)
        if item["status"] == "committed":
            current = object_value(object_value(item["receipt"])["index"])["current"]
    reservation = attempt(state, integer(release.media.state["revision"]))
    index = (
        {
            "index_format": 2,
            "revision": release.media.state["revision"],
            "current": release.entry,
            "previous": current,
        }
        if reservation["plan"] is None
        else object_value(reservation["plan"])["index"]
    )
    return {
        "mode": "offline_dry_run",
        "candidate_files": len(release.members) + len(release.assets) + 1,
        "candidate_bytes": sum(len(m.raw) for m in release.members)
        + sum(integer(a["bytes"]) for a in release.assets)
        + len(canonical(index)),
        "index_bytes": len(canonical(index)),
        "revision": release.media.state["revision"],
        "image_v_range": None if not versions else [min(versions), max(versions)],
        "new_v_range": [
            release.media.state["revision"],
            release.media.state["revision"],
        ]
        if release.media.state["revision"] in versions
        else None,
        "would_collect": [],
        "collection": "disabled_unverified_conditional_delete",
        "remote_existence": "not_checked",
        "manifest_sha256": release.entry["manifest_sha256"],
    }
