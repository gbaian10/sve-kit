"""Immutable synthetic file templates with private writable copies."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

type FrozenFiles = tuple[tuple[Path, bytes], ...]


def freeze_files(root: Path) -> FrozenFiles:
    return tuple(
        (path.relative_to(root), path.read_bytes())
        for path in sorted(root.rglob("*"))
        if path.is_file()
    )


def restore_files(files: FrozenFiles, root: Path) -> None:
    for relative, content in files:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
