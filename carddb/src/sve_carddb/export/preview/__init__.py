"""Isolated JP preview transport, deliberately separate from formal activation."""

import os
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value, string
from sve_carddb.export.publication import require_preview
from sve_carddb.export.read_api import POINTER
from sve_carddb.export.transport.measure import measure

if TYPE_CHECKING:
    from sve_carddb.export.transport import Brotli, Snapshot


@dataclass(frozen=True)
class Roots:
    preview: Path
    private: Path

    def verify(self) -> None:
        """Keep private outputs outside the public root that gets served or uploaded."""
        if not self.preview.is_absolute() or not self.private.is_absolute():
            raise ValueError("Preview and private roots must be absolute paths")
        preview, private = self.preview.resolve(), self.private.resolve()
        if preview.is_relative_to(private) or private.is_relative_to(preview):
            raise ValueError("Preview and private roots must be disjoint")

    def destination(self, relative: str, *, private: bool = False) -> Path:
        """Internal symlinks must not redirect a preview write outside its root."""
        self.verify()
        root = (self.private if private else self.preview).resolve()
        target = root / relative
        if not target.resolve().is_relative_to(root):
            raise ValueError("Preview destination escapes its root")
        return target


def write_artifact(
    roots: Roots, path: str, raw: bytes, *, immutable: bool, private: bool = False
) -> None:
    """Atomically write a confined artifact, preserving immutable bytes on retries."""
    target = roots.destination(path, private=private)
    target.parent.mkdir(parents=True, exist_ok=True)
    if immutable and target.exists():
        if target.read_bytes() != raw:
            raise ValueError("Immutable preview artifact differs from existing bytes")
        return
    # The pointer only changes after every immutable member has been sealed.
    descriptor, temporary = tempfile.mkstemp(dir=target.parent, prefix=".preview-")
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
        roots.destination(path, private=private)
        Path(temporary).replace(target)
    finally:
        Path(temporary).unlink(missing_ok=True)


def write_preview(
    snapshot: Snapshot,
    roots: Roots,
    provenance: dict[str, JsonValue],
    *,
    brotli: Brotli | None = None,
    regions: tuple[str, ...] = ("jp",),
) -> dict[str, JsonValue]:
    """Write a verified exporter snapshot and switch preview/current last."""
    roots.verify()
    require_preview(snapshot.manifest, regions=regions)
    descriptions = {
        string(object_value(item)["key"]): object_value(item)
        for item in array(snapshot.manifest["files"])
    }
    for key, blob in snapshot.payloads.items():
        path = string(descriptions[key]["path"])
        write_artifact(roots, path, blob.raw, immutable=True)
        write_artifact(roots, path + ".gz", blob.gzip, immutable=True)
        _write_brotli(roots, path, blob.br)
    manifest = snapshot.manifest_blob
    hashed = digest(manifest.raw)
    manifest_path = "snapshots/manifests/" + hashed[7:] + ".json"
    write_artifact(roots, manifest_path, manifest.raw, immutable=True)
    write_artifact(roots, manifest_path + ".gz", manifest.gzip, immutable=True)
    _write_brotli(roots, manifest_path, manifest.br)
    pointer: dict[str, JsonValue] = {
        "manifest_path": manifest_path,
        "manifest_sha256": hashed,
    }
    report = provenance | {
        "pointer": pointer,
        "capacity": measure(snapshot, brotli=brotli),
        "compression_recipe": dict(snapshot.compression_recipe),
    }
    write_artifact(
        roots,
        "reports/" + hashed[7:] + ".json",
        canonical(report),
        immutable=True,
        private=True,
    )
    write_artifact(roots, POINTER, canonical(pointer), immutable=False)
    return report


def _write_brotli(roots: Roots, path: str, raw: bytes | None) -> None:
    if raw is not None:
        write_artifact(roots, path + ".br", raw, immutable=True)
