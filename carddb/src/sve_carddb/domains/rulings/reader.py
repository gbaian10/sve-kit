"""Read each fixed ruling once without a legacy envelope or runtime Git lookup."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import ValidationError

from sve_carddb.contracts.rulings import Ruling, RulingSource
from sve_carddb.core.authored import read, require_directory
from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, parse_yaml

if TYPE_CHECKING:
    from pathlib import Path


@dataclass(frozen=True)
class Document:
    ruling: Ruling
    source: RulingSource
    raw: bytes


def document(path: str, raw: bytes) -> Document:
    """A malformed current document never becomes an unknown historical reference."""
    if len(raw) >= MAX_BYTES:
        raise ValueError(f"Oversized ruling: {path}")
    try:
        parsed = object_value(JSON_VALUE.validate_python(parse_yaml(raw), strict=True))
        content = canonical(parsed)
        ruling = Ruling.model_validate_json(content)
        source = RulingSource(path=path, sha256=digest(raw))
    except ValidationError as error:
        locations = [
            ".".join(map(str, item["loc"]))
            for item in error.errors(
                include_input=False, include_context=False, include_url=False
            )
        ]
        raise ValueError(
            f"Invalid versioned ruling document: {path}: loc={locations}"
        ) from None
    except ValueError, TypeError:
        raise ValueError(f"Invalid versioned ruling document: {path}") from None
    return Document(ruling, source, raw)


def load(repo: Path) -> tuple[Document, ...]:
    """Current IDs are unique even when two files claim different revisions."""
    root = repo / "authored"
    directory = root / "rules/rulings"
    require_directory(root, directory)
    result = tuple(
        document(path.relative_to(repo).as_posix(), read(path, root=root)[0])
        for path in sorted(directory.glob("*.yaml"))
    )
    if len({d.ruling.id for d in result}) != len(result):
        raise ValueError("Duplicate current ruling identity or version")
    return result
