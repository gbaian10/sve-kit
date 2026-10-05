"""Isolated JP preview transport, deliberately separate from formal activation."""

import os
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.export.compression import compress
from sve_carddb.snapshot.export.measure import measure
from sve_carddb.snapshot.preview.images import require_confirmed
from sve_carddb.snapshot.preview.media_state import commit
from sve_carddb.snapshot.publication import require_preview
from sve_carddb.snapshot.reader import read_snapshot, read_text_all
from sve_carddb.snapshot.values import array, canonical, digest, object_value, string

if TYPE_CHECKING:
    from sve_carddb.snapshot.export import Brotli, Snapshot
    from sve_carddb.snapshot.media import MediaPlan
    from sve_carddb.snapshot.project import Projection
    from sve_carddb.snapshot.project.source import Record


POINTER = "snapshots/preview/current.json"


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

    def verify_image_source(self, source: Path | None) -> None:
        """A copied library must never become a preview output directory."""
        if source is None:
            return
        root = source.resolve()
        for output in (self.preview.resolve(), self.private.resolve()):
            if output.is_relative_to(root) or root.is_relative_to(output):
                raise ValueError(
                    "Preview image input and output roots must be disjoint"
                )


def require_unknown_coverage(projection: Projection) -> None:
    """This input recipe contains no QA/errata/CR/restriction source coverage."""
    if (
        projection.metadata["source_windows"]
        or projection.metadata["restriction_coverage"]
    ):
        raise ValueError("Uncovered sources must remain empty windows / unknown")
    if any(
        projection.tables[table]
        for table in ("qa", "errata", "cr_version", "restriction")
    ):
        raise ValueError("Unrequested ancillary sources cannot become public facts")


def _write(
    roots: Roots, path: str, raw: bytes, *, immutable: bool, private: bool = False
) -> None:
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


def write_preview(  # ruff: ignore[too-many-arguments] -- the output boundary binds explicit region, roots, codec and image provenance
    snapshot: Snapshot,
    roots: Roots,
    provenance: dict[str, JsonValue],
    *,
    brotli: Brotli | None = None,
    image_source: Path | None = None,
    confirmed_images: frozenset[str] = frozenset(),
    regions: tuple[str, ...] = ("jp",),
    media_plan: MediaPlan | None = None,
) -> dict[str, JsonValue]:
    """Seal content-addressed transport and atomically switch only preview/current."""
    roots.verify()
    require_preview(snapshot.manifest, regions=regions)
    roots.verify_image_source(image_source)
    joined = read_snapshot(
        snapshot.manifest, {key: blob.raw for key, blob in snapshot.payloads.items()}
    )
    union_keys = {
        string(object_value(item)["key"])
        for item in array(object_value(snapshot.manifest["text_all"])["contains"])
    }
    if (
        read_text_all(
            snapshot.manifest,
            snapshot.text_all.raw,
            {
                key: blob.raw
                for key, blob in snapshot.payloads.items()
                if key not in union_keys
            },
        )
        != joined
    ):
        raise ValueError("Preview union and shards differ")
    descriptions = {
        string(object_value(item)["key"]): object_value(item)
        for item in array(snapshot.manifest["files"])
    }
    image_report = _publish_images(
        joined,
        roots,
        image_source,
        confirmed_images,
        media_plan,
    )
    for key, blob in snapshot.payloads.items():
        path = string(descriptions[key]["path"])
        if path != "snapshots/blobs/" + digest(blob.raw)[7:] + ".json":
            raise ValueError("Preview requires content-addressed blob paths")
        if digest(blob.raw) != descriptions[key]["sha256"]:
            raise ValueError("Preview blob differs from manifest hash")
        _write(roots, path, blob.raw, immutable=True)
        _write(roots, path + ".gz", blob.gzip, immutable=True)
        _write_brotli(roots, path, blob.br)
    union = object_value(snapshot.manifest["text_all"])
    if digest(snapshot.text_all.raw) != union["sha256"]:
        raise ValueError("Preview text union differs from manifest hash")
    union_path = string(union["path"])
    if union_path != "snapshots/blobs/" + digest(snapshot.text_all.raw)[7:] + ".json":
        raise ValueError("Preview requires content-addressed union path")
    _write(roots, union_path, snapshot.text_all.raw, immutable=True)
    _write(roots, union_path + ".gz", snapshot.text_all.gzip, immutable=True)
    _write_brotli(roots, union_path, snapshot.text_all.br)
    manifest = compress(canonical(snapshot.manifest), brotli)
    hashed = digest(manifest.raw)
    manifest_path = "snapshots/manifests/" + hashed[7:] + ".json"
    _write(roots, manifest_path, manifest.raw, immutable=True)
    _write(roots, manifest_path + ".gz", manifest.gzip, immutable=True)
    _write_brotli(roots, manifest_path, manifest.br)
    pointer: dict[str, JsonValue] = {
        "manifest_path": manifest_path,
        "manifest_sha256": hashed,
    }
    report = provenance | {
        "pointer": pointer,
        "capacity": measure(snapshot, brotli=brotli),
        "compression_recipe": dict(snapshot.compression_recipe),
        "images": image_report,
    }
    _write(
        roots,
        "reports/" + hashed[7:] + ".json",
        canonical(report),
        immutable=True,
        private=True,
    )
    commit(roots, _verify_written_images(roots, media_plan).state)
    _write(roots, POINTER, canonical(pointer), immutable=False)
    return report


def _write_brotli(roots: Roots, path: str, raw: bytes | None) -> None:
    if raw is not None:
        _write(roots, path + ".br", raw, immutable=True)


def _verify_written_images(roots: Roots, media_plan: MediaPlan | None) -> MediaPlan:
    if media_plan is None:
        raise ValueError("Preview requires the matching verified media plan")
    for asset in media_plan.assets:
        raw = roots.destination(string(asset["path"])).read_bytes()
        if digest(raw) != asset["sha256"] or len(raw) != asset["bytes"]:
            raise ValueError("Written media differs from sealed plan")
    return media_plan


def _publish_images(
    tables: dict[str, list[Record]],
    roots: Roots,
    image_source: Path | None,
    confirmed_images: frozenset[str],
    media_plan: MediaPlan | None,
) -> dict[str, JsonValue]:
    if media_plan is None or media_plan.projection.tables != tables:
        raise ValueError("Preview requires the matching verified media plan")
    require_confirmed(tables, confirmed_images)
    if media_plan.assets and image_source is None:
        raise ValueError("Preview media requires an explicit asset source")
    image_files, image_bytes = 0, 0
    if image_source is not None:
        for path, raw in media_plan.blobs(image_source):
            target = roots.destination(path)
            if target.exists() and target.read_bytes() != raw:
                # The old pointer's versions no longer describe these bytes.
                roots.destination(POINTER).unlink(missing_ok=True)
            _write(roots, path, raw, immutable=False)
            image_files += 1
            image_bytes += len(raw)
    return {"unique_files": image_files, "unique_bytes": image_bytes}
