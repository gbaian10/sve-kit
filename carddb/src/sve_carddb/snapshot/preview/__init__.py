"""Isolated JP preview transport, deliberately separate from formal activation."""

import os
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.export.compression import compress
from sve_carddb.snapshot.export.measure import measure
from sve_carddb.snapshot.preview.images import image_blobs, verify_images
from sve_carddb.snapshot.publication import require_preview
from sve_carddb.snapshot.reader import read_snapshot, read_text_all
from sve_carddb.snapshot.values import array, canonical, digest, object_value, string

if TYPE_CHECKING:
    from sve_carddb.snapshot.export import Brotli, Snapshot
    from sve_carddb.snapshot.project import Projection
    from sve_carddb.snapshot.project.source import Record


@dataclass(frozen=True)
class Roots:
    preview: Path
    formal: Path

    def verify(self) -> None:
        """Resolve symlinks before checking both containment directions."""
        if not self.preview.is_absolute():
            raise ValueError("Preview root must be an absolute path")
        preview, formal = self.preview.resolve(), self.formal.resolve()
        if preview.is_relative_to(formal) or formal.is_relative_to(preview):
            raise ValueError("Preview and formal roots must be disjoint")

    def destination(self, relative: str) -> Path:
        """Internal symlinks must not turn a preview write into a formal write."""
        self.verify()
        root = self.preview.resolve()
        target = root / relative
        if not target.resolve().is_relative_to(root):
            raise ValueError("Preview destination escapes its root")
        return target

    def verify_image_source(self, source: Path | None) -> None:
        """A copied library must never become an output or formal asset directory."""
        if source is None:
            return
        for output in (self.preview.resolve(), self.formal.resolve()):
            root = source.resolve()
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


def _write(roots: Roots, path: str, raw: bytes, *, immutable: bool) -> None:
    target = roots.destination(path)
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
        roots.destination(path)
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
    image_report = _write_images(joined, roots, image_source, confirmed_images)
    for key, blob in snapshot.payloads.items():
        path = string(descriptions[key]["path"])
        if path != "snapshots/blobs/" + digest(blob.raw)[7:] + ".json":
            raise ValueError("Preview requires content-addressed blob paths")
        if digest(blob.raw) != descriptions[key]["sha256"]:
            raise ValueError("Preview blob differs from manifest hash")
        _write(roots, path, blob.raw, immutable=True)
        _write(roots, path + ".gz", blob.gzip, immutable=True)
        if blob.br is not None:
            _write(roots, path + ".br", blob.br, immutable=True)
    union = object_value(snapshot.manifest["text_all"])
    if digest(snapshot.text_all.raw) != union["sha256"]:
        raise ValueError("Preview text union differs from manifest hash")
    union_path = string(union["path"])
    if union_path != "snapshots/blobs/" + digest(snapshot.text_all.raw)[7:] + ".json":
        raise ValueError("Preview requires content-addressed union path")
    _write(roots, union_path, snapshot.text_all.raw, immutable=True)
    _write(roots, union_path + ".gz", snapshot.text_all.gzip, immutable=True)
    if snapshot.text_all.br is not None:
        _write(roots, union_path + ".br", snapshot.text_all.br, immutable=True)
    manifest = compress(canonical(snapshot.manifest), brotli)
    hashed = digest(manifest.raw)
    manifest_path = "snapshots/manifests/" + hashed[7:] + ".json"
    _write(roots, manifest_path, manifest.raw, immutable=True)
    _write(roots, manifest_path + ".gz", manifest.gzip, immutable=True)
    if manifest.br is not None:
        _write(roots, manifest_path + ".br", manifest.br, immutable=True)
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
    _write(roots, "reports/" + hashed[7:] + ".json", canonical(report), immutable=True)
    verify_images(joined, roots.preview, confirmed_images)
    _write(roots, "snapshots/preview/current.json", canonical(pointer), immutable=False)
    return report


def _write_images(
    tables: dict[str, list[Record]],
    roots: Roots,
    image_source: Path | None,
    confirmed_images: frozenset[str],
) -> dict[str, JsonValue]:
    image_files, image_bytes = 0, 0
    for path, raw in image_blobs(tables, image_source, confirmed_images):
        _write(roots, path, raw, immutable=True)
        image_files += 1
        image_bytes += len(raw)
    return {"unique_files": image_files, "unique_bytes": image_bytes}
