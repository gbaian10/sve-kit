"""Create-only isolated registration; neither the crawler nor settings are opened."""

import hashlib
import os
import shutil
import stat
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- Only local Git plumbing is used to verify immutable code pins.
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.html import parse as html_parse
from sve_carddb.html import select_one
from sve_carddb.manifest import Manifest, ManifestError
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.source_archive import _fsync_dir, _rename_no_replace
from sve_carddb.source_import.models import (
    Content,
    Mapping,
    Selection,
    check_purpose,
    load_index,
    official_url,
    raw_path,
    receipt_id,
)

if TYPE_CHECKING:
    from sve_carddb.source_import.models import Observation

_METADATA_LIMIT = 1024 * 1024
_RAW_LIMIT = 64 * 1024 * 1024
_REPO = Path(__file__).resolve().parents[4]
PROGRAM_FILES = (
    "carddb/pyproject.toml",
    "carddb/src/sve_carddb/build_db/domains.py",
    "carddb/src/sve_carddb/build_inputs.py",
    "carddb/src/sve_carddb/cli.py",
    "carddb/src/sve_carddb/html.py",
    "carddb/src/sve_carddb/manifest.py",
    "carddb/src/sve_carddb/registry/records.py",
    "carddb/src/sve_carddb/snapshot/values.py",
    "carddb/src/sve_carddb/source_archive.py",
    "carddb/src/sve_carddb/source_import/__init__.py",
    "carddb/src/sve_carddb/source_import/commands.py",
    "carddb/src/sve_carddb/source_import/importer.py",
    "carddb/src/sve_carddb/source_import/models.py",
    "carddb/src/sve_carddb/store.py",
    "carddb/src/sve_carddb/urls.py",
    "carddb/uv.lock",
)


class SourceImportError(ValueError):
    """Acquisition or publication cannot be verified without changing another task."""


@dataclass(frozen=True, slots=True)
class Plan:
    input_dir: Path
    selection_path: Path
    index: bytes
    selection_bytes: bytes
    content: Content

    @property
    def id(self) -> str:
        """Identify all observed metadata and classifications, not the current clock."""
        return receipt_id(self.index, self.content)

    def report(self) -> dict[str, str | int]:
        """Counts are safe to print without returning any official source content."""
        return {
            "receipt_id": self.id,
            "files": len(self.content.source_mappings),
            "bytes": sum(item.bytes for item in load_index(self.index)),
        }


def read_regular(path: Path, limit: int = _METADATA_LIMIT) -> bytes:
    """Walk by directory FDs so a replaced parent symlink cannot escape the input."""
    absolute = path.absolute()
    if ".." in absolute.parts:
        raise SourceImportError("Source import paths must not contain parent traversal")
    fd = os.open(absolute.anchor, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in absolute.parts[1:-1]:
            following = os.open(
                part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd
            )
            os.close(fd)
            fd = following
        source = os.open(
            absolute.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd
        )
        with os.fdopen(source, "rb") as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
                raise SourceImportError(
                    "Source import input must be a bounded regular file"
                )
            data = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
            if _identity(before) != _identity(after) or len(data) != before.st_size:
                raise SourceImportError("Source import input changed during reading")
            return data
    finally:
        os.close(fd)


def _identity(value: os.stat_result) -> tuple[int, ...]:
    return (
        value.st_dev,
        value.st_ino,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
        value.st_mode,
    )


def _body(raw: bytes, observation: Observation) -> None:
    if (
        len(raw) != observation.bytes
        or hashlib.sha256(raw).hexdigest() != observation.sha256
    ):
        raise SourceImportError("Source import raw hash or bytes differ from the index")
    if observation.media_type == "application/pdf":
        if not raw.startswith(b"%PDF-") or not raw.rstrip().endswith(b"%%EOF"):
            raise SourceImportError("Source import raw is not a complete PDF container")
    else:
        document = html_parse(raw.decode("utf-8"))
        title = select_one(document, "title")
        body = select_one(document, "body")
        if (
            title is None
            or body is None
            or not title.text().strip()
            or not body.text().strip()
        ):
            raise SourceImportError("Source import raw is not a nonempty HTML document")
        if title.text().strip().lower() in {
            "404",
            "404 not found",
            "not found",
            "error",
        }:
            raise SourceImportError("Source import HTML is an error page")


def prepare(input_dir: Path, selection_path: Path) -> Plan:
    """Validate the complete observed set before opening any output manifest."""
    index = read_regular(input_dir / "index.jsonl")
    selection_bytes = read_regular(selection_path)
    selection = Selection.model_validate_json(canonical(parse(selection_bytes)))
    observations = {official_url(row.url): row for row in load_index(index)}
    purposes = {item.url: item for item in selection.sources}
    if purposes.keys() != observations.keys():
        raise SourceImportError("Source selection differs from the complete index")
    raw_dir = input_dir / "raw"
    if raw_dir.is_symlink() or {item.name for item in raw_dir.iterdir()} != {
        row.sha256 for row in observations.values()
    }:
        raise SourceImportError("Source import raw set differs from the complete index")
    mappings = []
    for url in sorted(purposes):
        observation = observations[url]
        purpose = purposes[url]
        check_purpose(purpose, observation)
        raw = read_regular(input_dir / "raw" / observation.sha256, _RAW_LIMIT)
        _body(raw, observation)
        mappings.append(
            Mapping(
                url=url,
                provider=purpose.provider,
                kind=purpose.kind,
                raw_hash="sha256:" + observation.sha256,
                path=raw_path(purpose, observation),
            )
        )
    return Plan(
        input_dir,
        selection_path,
        index,
        selection_bytes,
        Content(
            source_mappings=tuple(mappings),
            program_revision=selection.program_revision,
            dependencies=selection.dependencies,
        ),
    )


def _git(root: Path, *args: str) -> bytes:
    try:
        return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- Git plumbing is local and cannot interpret shell text.
            [shutil.which("git") or "/usr/bin/git", "-C", str(root), *args],
            check=True,
            capture_output=True,
        ).stdout
    except subprocess.CalledProcessError as exc:
        raise SourceImportError(
            "Source import program pins cannot be resolved from the supplied repository"
        ) from exc


def verify_program(plan: Plan, root: Path) -> None:
    """Pin actual executing code and lockfiles, not just caller-supplied hash strings."""
    if (
        _git(root, "rev-parse", "HEAD").decode().strip()
        != plan.content.program_revision
    ):
        raise SourceImportError(
            "Source import program revision differs from the repository HEAD"
        )
    pins = {pin.name: pin.sha256 for pin in plan.content.dependencies}
    if set(pins) != set(PROGRAM_FILES):
        raise SourceImportError(
            "Source import program dependency set is incomplete or unknown"
        )
    for name in PROGRAM_FILES:
        content = read_regular(_REPO / name)
        if (
            pins[name] != digest(content)
            or _git(root, "show", f"{plan.content.program_revision}:{name}") != content
        ):
            raise SourceImportError(
                "Source import program dependency differs from executing code"
            )


def check_destination(plan: Plan, output: Path) -> None:
    """Refuse input/live overlap before looking at any existing manifest."""
    if not output.is_absolute() or output != output.resolve():
        raise SourceImportError(
            "Source import output must be absolute without symlinks or traversal"
        )
    protected = [plan.input_dir.resolve(), plan.selection_path.resolve()]
    live = os.environ.get("SVE_DATA_DIR")
    if live:
        protected.append(Path(live).resolve())
    if any(
        output.is_relative_to(path) or path.is_relative_to(output) for path in protected
    ):
        raise SourceImportError(
            "Source import output overlaps inputs or configured live data"
        )


def _recheck(plan: Plan) -> None:
    if prepare(plan.input_dir, plan.selection_path) != plan:
        raise SourceImportError("Source import inputs changed after preparation")


def _reuse(plan: Plan, output: Path) -> dict[str, str | int]:
    if read_regular(output / ".source-import") != plan.id.encode():
        raise SourceImportError("Existing output is not the exact isolated import")
    db = output / "manifest" / "manifest.sqlite"
    read_regular(db, _RAW_LIMIT)
    with Manifest.open_snapshot(db) as manifest:
        receipt = manifest.source_import_receipt()
        if (
            receipt is None
            or receipt.receipt_id != plan.id
            or receipt.index_bytes != plan.index
        ):
            raise SourceImportError(
                "Existing import manifest differs from the planned receipt"
            )
    observations = {official_url(item.url): item for item in load_index(plan.index)}
    for mapping in plan.content.source_mappings:
        _body(
            read_regular(output / mapping.path, _RAW_LIMIT), observations[mapping.url]
        )
    return plan.report() | {"state": "reused"}


def _write(path: Path, raw: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    _fsync_dir(path.parent)


def register(plan: Plan, output: Path, program_root: Path) -> dict[str, str | int]:
    """Publish a complete v2 dataset once; interruption never repairs existing work."""
    check_destination(plan, output)
    verify_program(plan, program_root)
    _recheck(plan)
    if output.exists():
        return _reuse(plan, output)
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".source-import-", dir=output.parent))
    try:
        observations = {official_url(item.url): item for item in load_index(plan.index)}
        for mapping in plan.content.source_mappings:
            raw = read_regular(
                plan.input_dir / "raw" / mapping.raw_hash[7:], _RAW_LIMIT
            )
            _body(raw, observations[mapping.url])
            _write(stage / mapping.path, raw)
        target = stage / "manifest" / "manifest.sqlite"
        target.parent.mkdir()
        with Manifest.create_import(target, plan.index, plan.content) as manifest:
            if manifest.integrity_check() != "ok":
                raise ManifestError("New import manifest failed integrity check")
        with target.open("rb") as stream:
            os.fsync(stream.fileno())
        _fsync_dir(target.parent)
        _write(stage / ".source-import", plan.id.encode())
        for directory in sorted(
            (item for item in stage.rglob("*") if item.is_dir()), reverse=True
        ):
            _fsync_dir(directory)
        _fsync_dir(stage)
        _recheck(plan)
        _rename_no_replace(stage, output)
        _fsync_dir(output.parent)
    finally:
        if stage.exists():
            shutil.rmtree(stage)
    return plan.report() | {"state": "registered"}
