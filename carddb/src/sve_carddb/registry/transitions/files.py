"""Read complete transition files without treating parsed input as an effective registry."""

import re
from dataclasses import dataclass
from pathlib import Path

from pydantic import JsonValue

from sve_carddb.registry.inputs import JSON_VALUE, canonical, digest
from sve_carddb.registry.records import RecordData
from sve_carddb.registry.transitions.models import Shard
from sve_carddb.registry.yaml_reader import parse_yaml

MAX_BYTES = 1_048_576
_PATH = re.compile(r"identity-transitions/([0-9]{3,})\.yaml")


@dataclass(frozen=True)
class LoadedShard:
    path: str
    content_hash: str
    content: bytes
    exact_content: bytes

    def envelope(self) -> Shard:
        """Return detached data, preserving both exact YAML and canonical bytes."""
        return Shard.model_validate_json(self.content)


@dataclass(frozen=True)
class TransitionFiles:
    shards: tuple[LoadedShard, ...]


def checked_model[T: RecordData](model: type[T], raw: JsonValue) -> T:
    """Hide imported data in Pydantic errors while exposing a stable boundary error."""
    try:
        checked = model.model_validate_json(canonical(raw))
    except ValueError:
        raise ValueError("Invalid identity transition authored fields") from None
    if canonical(checked.model_dump(mode="json", round_trip=True)) != canonical(raw):
        raise ValueError("Identity transition normalization changed canonical input")
    return checked


def _safe_file(root: Path, path: Path) -> None:
    for component in (path, *path.parents):
        if component.is_symlink():
            raise ValueError("Symlinks are forbidden in identity transition inputs")
        if component == root:
            break
    if not path.is_file():
        raise ValueError("Missing identity transition input file")


def _read(path: Path) -> tuple[JsonValue, bytes]:
    exact = path.read_bytes()
    if len(exact) >= MAX_BYTES:
        raise ValueError("Identity transition YAML must be smaller than 1 MiB")
    try:
        raw = JSON_VALUE.validate_python(parse_yaml(exact), strict=True)
        canonical(raw)
    except ValueError, TypeError:
        raise ValueError("Invalid identity transition YAML") from None
    return raw, exact


def _inventory(root: Path) -> list[str]:
    directory = root / "identity-transitions"
    sequences: dict[int, str] = {}
    for file in directory.rglob("*"):
        if file.is_symlink():
            raise ValueError("Symlinks are forbidden in identity transition inputs")
        if file.is_dir():
            continue
        name = file.relative_to(root).as_posix()
        match = _PATH.fullmatch(name)
        if match is None:
            raise ValueError("Unexpected identity transition input path")
        sequence = int(match[1])
        if sequence in sequences:
            raise ValueError("Duplicate identity transition sequence")
        sequences[sequence] = name
    if sorted(sequences) != list(range(1, len(sequences) + 1)):
        raise ValueError("Identity transition sequences must be contiguous from 1")
    return [sequences[sequence] for sequence in sorted(sequences)]


def read_transition_files(root: Path) -> TransitionFiles:
    """Validate paths, sequence numbers and strict wire shapes, without applying them.

    An absent directory has no transitions. The caller must hold the shared
    registry lock or supply a stable immutable authored checkout.
    """
    directory = root / "identity-transitions"
    if directory.is_symlink():
        raise ValueError("Symlinks are forbidden in identity transition inputs")
    if not directory.exists():
        return TransitionFiles(())
    shards = []
    for name in _inventory(root):
        path = root / name
        _safe_file(root, path)
        raw, exact = _read(path)
        shard = checked_model(Shard, raw)
        if shard.records[0].sequence != int(Path(name).stem):
            raise ValueError("Identity transition sequence disagrees with shard path")
        shards.append(LoadedShard(name, digest(raw), canonical(raw), exact))
    return TransitionFiles(tuple(shards))


def require_empty_transitions(root: Path) -> None:
    """Stop legacy build and append paths from silently ignoring identity changes."""
    if read_transition_files(root).shards:
        raise ValueError(
            "Nonempty identity transitions require effective projection support"
        )
