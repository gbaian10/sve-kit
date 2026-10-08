"""Read bounded authored YAML from explicit data areas in the working tree."""

import re
from typing import TYPE_CHECKING

from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.storage import MAX_BYTES
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical

if TYPE_CHECKING:
    from pathlib import Path


def read(path: Path) -> tuple[bytes, bytes]:
    """Reject links before reading so authored inputs cannot escape their data area."""
    if not path.is_file() or any(p.is_symlink() for p in (path, *path.parents)):
        raise ValueError("Missing or symlink authored input")
    raw = path.read_bytes()
    if len(raw) >= MAX_BYTES:
        raise ValueError("Authored input must be smaller than one MiB")
    try:
        content = canonical(JSON_VALUE.validate_python(parse_yaml(raw), strict=True))
    except ValueError, TypeError:
        raise ValueError("Invalid authored YAML input") from None
    return raw, content


def shards(root: Path, areas: tuple[str, ...]) -> tuple[tuple[str, bytes, bytes], ...]:
    """Only numeric YAML shards in known data directories enter the build."""
    result = []
    for area in areas:
        directory = root / area
        if any(p.is_symlink() for p in (directory, *directory.parents)):
            raise ValueError("Symlink authored data area")
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
                raw, content = read(path)
                result.append((path.relative_to(root).as_posix(), raw, content))
    return tuple(sorted(result))
