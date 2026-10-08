"""Read bounded authored YAML from explicit data areas in the working tree."""

import re
from typing import TYPE_CHECKING

from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.storage import MAX_BYTES
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical

if TYPE_CHECKING:
    from pathlib import Path


def check_path(root: Path, path: Path) -> None:
    """Only links within authored data can redirect a declared build input."""
    relative = path.relative_to(root)
    if ".." in relative.parts:
        raise ValueError("Authored input is outside its data root")
    current = root
    for part in ("", *relative.parts):
        current /= part
        if current.is_symlink():
            raise ValueError("Symlink authored data area")


def require_directory(root: Path, path: Path) -> None:
    """Missing declared areas are errors unless the caller explicitly opts out."""
    check_path(root, path)
    if not path.is_dir():
        raise ValueError("Missing authored data area")


def read(path: Path, *, root: Path) -> tuple[bytes, bytes]:
    """Reject links before reading so authored inputs cannot escape their data area."""
    check_path(root, path)
    if not path.is_file():
        raise ValueError("Missing or symlink authored input")
    raw = path.read_bytes()
    if len(raw) >= MAX_BYTES:
        raise ValueError("Authored input must be smaller than one MiB")
    try:
        content = canonical(JSON_VALUE.validate_python(parse_yaml(raw), strict=True))
    except ValueError, TypeError:
        raise ValueError("Invalid authored YAML input") from None
    return raw, content


def shards(
    root: Path, areas: tuple[str, ...], *, optional: tuple[str, ...] = ()
) -> tuple[tuple[str, bytes, bytes], ...]:
    """Only numeric YAML shards in known data directories enter the build."""
    require_directory(root, root)
    result = []
    for area in areas:
        directory = root / area
        check_path(root, directory)
        if area in optional and not directory.exists():
            continue
        require_directory(root, directory)
        for group in sorted(directory.glob("*")):
            if group.is_symlink():
                raise ValueError("Symlink authored shard group")
            if (
                not group.is_dir()
                or re.fullmatch(r"[A-Za-z0-9_-]+", group.name) is None
            ):
                continue
            for path in sorted(group.glob("*.yaml")):
                if re.fullmatch(r"[0-9]{3,}\.yaml", path.name) is None:
                    continue
                raw, content = read(path, root=root)
                result.append((path.relative_to(root).as_posix(), raw, content))
    return tuple(sorted(result))
