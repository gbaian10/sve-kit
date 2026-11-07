"""Upload one scanner release (docs/schema/scan/artifacts.md): hashed files first, manifest last."""

import hashlib
import json
import re
from dataclasses import dataclass
from functools import partial
from math import isfinite
from typing import TYPE_CHECKING

from sve_carddb.export.read_api import ExportError, directory, read_member

from sve_publish.adapter import SCAN_PREFIXES
from sve_publish.headers import INDEX_HEADERS, JSON_HEADERS, SCAN_BINARY_HEADERS
from sve_publish.parallel import UploadObject, UploadPool

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from sve_publish.adapter import R2Store

FORMAT = 1
MANIFEST = "scan/manifest.json"
EDITIONS = ("jp", "en")
INDEX_PARTS = {"vectors": ".i8", "scale": ".scale", "hub": ".hub", "cards": ".json"}
SHA256 = re.compile(r"[0-9a-f]{64}")
FLOAT32 = 4
REC_INPUT_SIDES = 2  # [height, width]

type Json = dict[str, object]


@dataclass(frozen=True)
class ScanFile:
    """`file` is relative to the release folder; the R2 key adds the `scan/` prefix."""

    file: str
    bytes: int
    sha256: str

    @property
    def key(self) -> str:
        """Published object key."""
        return "scan/" + self.file


@dataclass(frozen=True, repr=False)
class Release:
    """A release folder whose manifest and every referenced file were verified."""

    root: Path
    manifest: bytes
    det: int
    rec: int
    index: dict[str, Json]
    files: tuple[ScanFile, ...]


def _object(value: object, at: str) -> Json:
    if not isinstance(value, dict):
        raise ExportError(f"Scan manifest {at} must be an object")
    return value


def _positive(value: object, at: str) -> int:
    if type(value) is not int or value <= 0:
        raise ExportError(f"Scan manifest {at} must be a positive integer")
    return value


def _number(value: object, at: str) -> float:
    if (
        isinstance(value, bool)
        or not isinstance(value, int | float)
        or not isfinite(value)
    ):
        raise ExportError(f"Scan manifest {at} must be a finite number")
    return float(value)


def _read(root: Path, file: str, at: str) -> bytes:
    try:
        return read_member(root, file)
    except OSError:
        raise ExportError(f"Scan {at} file is missing: {file}") from None


def _file(root: Path, value: object, at: str, folder: str, suffix: str) -> ScanFile:
    """The file name must be the declared hash, and the bytes must hash to it."""
    row = _object(value, at)
    sha256 = row.get("sha256")
    if not isinstance(sha256, str) or not SHA256.fullmatch(sha256):
        raise ExportError(f"Scan manifest {at}.sha256 is not a SHA-256")
    size = _positive(row.get("bytes"), f"{at}.bytes")
    file = f"{folder}/{sha256}{suffix}"
    if row.get("file") != file:
        raise ExportError(f"Scan manifest {at}.file must be {folder}/<sha256>{suffix}")
    raw = _read(root, file, at)
    if len(raw) != size:
        raise ExportError(f"Scan {at} size differs from the manifest: {file}")
    if hashlib.sha256(raw).hexdigest() != sha256:
        raise ExportError(f"Scan {at} SHA-256 differs from the manifest: {file}")
    return ScanFile(file, size, sha256)


def _det(root: Path, value: object) -> tuple[int, ScanFile]:
    det = _object(value, "det")
    version = _positive(det.get("version"), "det.version")
    side = _positive(det.get("input"), "det.input")
    if side % _positive(det.get("stride"), "det.stride"):
        raise ExportError("Scan manifest det.input must be a multiple of det.stride")
    if _number(det.get("grow"), "det.grow") <= 0:
        raise ExportError("Scan manifest det.grow must be positive")
    return version, _file(root, det, "det", "models", ".onnx")


def _rec(root: Path, value: object) -> tuple[int, int, ScanFile]:
    rec = _object(value, "rec")
    version = _positive(rec.get("version"), "rec.version")
    shape = rec.get("input")
    if not isinstance(shape, list) or len(shape) != REC_INPUT_SIDES:
        raise ExportError("Scan manifest rec.input must be [height, width]")
    for side in shape:
        _positive(side, "rec.input")
    dim = _positive(rec.get("dim"), "rec.dim")
    scoring = _object(rec.get("scoring"), "rec.scoring")
    for name in ("hubWeight", "accept", "margin"):
        _number(scoring.get(name), f"rec.scoring.{name}")
    return version, dim, _file(root, rec, "rec", "models", ".onnx")


def _index(
    root: Path, edition: str, value: object, rec: int, dim: int
) -> tuple[Json, list[ScanFile]]:
    at = f"index.{edition}"
    row = _object(value, at)
    if _positive(row.get("rec"), f"{at}.rec") != rec:
        raise ExportError(f"Scan manifest {at} was built for another recogniser")
    data_version = row.get("data_version")
    if not isinstance(data_version, str) or not data_version:
        raise ExportError(f"Scan manifest {at}.data_version is missing")
    count = _positive(row.get("count"), f"{at}.count")
    files = {
        part: _file(root, row.get(part), f"{at}.{part}", "index", suffix)
        for part, suffix in INDEX_PARTS.items()
    }
    # One int8 row of `dim` values, then one float32 scale and hub value per card.
    if (files["vectors"].bytes, files["scale"].bytes, files["hub"].bytes) != (
        count * dim,
        count * FLOAT32,
        count * FLOAT32,
    ):
        raise ExportError(f"Scan manifest {at} sizes do not match its card count")
    return {"data_version": data_version, "count": count}, list(files.values())


def _not_json(constant: str) -> object:
    raise ValueError(constant)


def load_release(root: Path) -> Release:
    """Refuse anything the site reader would reject or that differs from its manifest."""
    directory(root)
    raw = _read(root, "manifest.json", "manifest")
    try:
        # The site's JSON.parse rejects the whole file on NaN or Infinity, wherever they appear.
        value = json.loads(raw.decode("utf-8"), parse_constant=_not_json)
    except UnicodeDecodeError, ValueError:
        raise ExportError("Scan manifest is not JSON") from None
    manifest = _object(value, "root")
    if type(manifest.get("format")) is not int or manifest["format"] != FORMAT:
        raise ExportError(f"Scan manifest format must be {FORMAT}")
    det, det_file = _det(root, manifest.get("det"))
    rec, dim, rec_file = _rec(root, manifest.get("rec"))
    editions = _object(manifest.get("index"), "index")
    if unknown := set(editions) - set(EDITIONS):
        raise ExportError(f"Scan manifest has unknown editions: {sorted(unknown)}")
    files = {det_file.key: det_file, rec_file.key: rec_file}
    index: dict[str, Json] = {}
    for edition in sorted(editions):
        index[edition], parts = _index(root, edition, editions[edition], rec, dim)
        files |= {item.key: item for item in parts}
    return Release(root, raw, det, rec, index, tuple(files.values()))


def report(release: Release) -> dict[str, object]:
    """Local facts only; which files the bucket already holds is unknown offline."""
    return {
        "mode": "dry_run",
        "format": FORMAT,
        "det": f"det-{release.det}",
        "rec": f"rec-{release.rec}",
        "index": release.index,
        "files": len(release.files),
        "bytes": sum(item.bytes for item in release.files),
        "manifest_sha256": hashlib.sha256(release.manifest).hexdigest(),
        "remote": "not_checked",
    }


def _verified(root: Path, item: ScanFile) -> bytes:
    """Reread at PUT time, so a file changed after validation is never published."""
    raw = _read(root, item.file, "release")
    if len(raw) != item.bytes or hashlib.sha256(raw).hexdigest() != item.sha256:
        raise ExportError("Scan file changed after validation")
    return raw


def upload_scan(
    store: R2Store,
    release: Release,
    *,
    jobs: int = 16,
    progress: Callable[[str], None] | None = None,
) -> dict[str, object]:
    """A failed run leaves the old manifest current; rerunning the same release resumes."""
    if jobs < 1:
        raise ExportError("Upload jobs must be at least 1")
    before = store.get(MANIFEST)
    # Names are content hashes written create-only by this tool, so a listed key is complete.
    stored = {key for prefix in sorted(SCAN_PREFIXES) for key in store.keys(prefix)}
    missing = [item for item in release.files if item.key not in stored]
    pool = UploadPool(store, jobs, len(missing), progress)
    pool.sync(
        UploadObject(
            item.key,
            partial(_verified, release.root, item),
            JSON_HEADERS if item.file.endswith(".json") else SCAN_BINARY_HEADERS,
        )
        for item in missing
    )
    current = (
        before is not None
        and before.raw == release.manifest
        and before.headers == INDEX_HEADERS
    )
    if not current:
        if store.get(MANIFEST) != before:
            raise ExportError("Scan manifest changed during upload")
        if not store.put(
            MANIFEST,
            release.manifest,
            INDEX_HEADERS,
            expected=None if before is None else before.etag,
        ):
            raise ExportError("Scan manifest conditional write failed")
    final = store.get(MANIFEST)
    if final is None or final.raw != release.manifest:
        raise ExportError("Scan manifest read-back differs from this release")
    return {
        "mode": "execute",
        "written_files": pool.written,
        "unchanged_files": len(release.files) - pool.written,
        "manifest": "unchanged" if current else "updated",
        "cdn_verification": "not_checked",
    }
