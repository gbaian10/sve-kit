"""Read current template inputs from fixed working-tree data areas."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.authored_files import shards
from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical

if TYPE_CHECKING:
    from pathlib import Path

SHARD = re.compile(
    r"translations/(glossary|overrides|templates)/([A-Za-z0-9_-]+)/([0-9]{3,})\.yaml\Z"
)
LIMIT = 1048576


def json_bytes(raw: bytes) -> bytes:
    """One strict YAML document, bounded before parsing; errors cannot print source values."""
    if len(raw) >= LIMIT:
        raise ValueError("Template input must be smaller than one MiB")
    try:
        return canonical(JSON_VALUE.validate_python(parse_yaml(raw), strict=True))
    except ValueError, TypeError:
        raise ValueError("Invalid template YAML input") from None


@dataclass(frozen=True)
class Files:
    revision: str
    content: tuple[tuple[str, bytes, bytes], ...]


def read(root: Path, commit: str) -> Files:
    """The commit is descriptive; current authored files supply the input values."""
    return Files(
        commit,
        shards(
            root / "authored",
            (
                "translations/glossary",
                "translations/overrides",
                "translations/templates",
            ),
        ),
    )
