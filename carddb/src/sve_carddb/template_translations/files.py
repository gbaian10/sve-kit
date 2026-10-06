"""Read the complete current translation tree from Git, without traversing ancestors."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import ValidationError

from sve_carddb.registry.inputs import JSON_VALUE
from sve_carddb.registry.yaml_reader import parse_yaml
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameter_rules.repository import git, revision
from sve_carddb.translations.models import Index

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository

INDEX = "authored/translations/index.yaml"
DIRECTORY = "authored/translations"
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
    index: bytes
    content: tuple[tuple[str, bytes, bytes], ...]


def tree(repository: PinnedRepository, commit: str) -> dict[str, str]:
    """Reject unsafe tree modes and filenames before reading any blob."""
    revision(repository, commit)
    result = {}
    for row in git(
        repository, "ls-tree", "-r", "-l", "-z", commit, "--", "authored"
    ).split(b"\0"):
        if not row:
            continue
        header, separator, encoded = row.partition(b"\t")
        try:
            name = encoded.decode("utf-8")
            mode, kind, oid, size = header.split()
        except UnicodeError, ValueError:
            raise ValueError("Invalid template Git tree entry") from None
        if name not in {"authored", DIRECTORY} and not name.startswith(DIRECTORY + "/"):
            continue
        relative = name.removeprefix("authored/")
        supported = name == INDEX or SHARD.fullmatch(relative) is not None
        regular = mode == b"100644" and kind == b"blob" and size.isdigit()
        if not separator or not regular or not supported or int(size) >= LIMIT:
            raise ValueError(
                "Template Git closure contains unsafe or unsupported files"
            )
        result[name] = oid.decode("ascii")
    return result


def _index(raw: bytes) -> Index:
    try:
        return Index.model_validate_json(json_bytes(raw))
    except ValidationError:
        raise ValueError("Invalid template translation index") from None


def read(repository: PinnedRepository, commit: str) -> Files:
    """Exact Git modes and the indexed closure must agree before any projection."""
    names = tree(repository, commit)
    if INDEX not in names:
        raise ValueError("Template translation index must explicitly exist")
    raw = repository.read_many(commit, tuple(sorted(names)))
    index = _index(raw[INDEX])
    indexed = index.includes
    if set(names) != {INDEX, *("authored/" + p for p in indexed)}:
        raise ValueError("Template indexed file closure differs from Git")
    sequences: dict[str, list[int]] = {}
    content = []
    for path, checksum in sorted(indexed.items()):
        match = SHARD.fullmatch(path)
        if match is None:
            raise ValueError("Template indexed path is unsafe or in the wrong area")
        number = int(match.groups()[-1])
        sequences.setdefault(path.rsplit("/", 1)[0], []).append(number)
        exact = raw["authored/" + path]
        parsed = json_bytes(exact)
        if digest(parsed) != checksum:
            raise ValueError("Template indexed input hash mismatch")
        content.append((path, exact, parsed))
    if any(sorted(ns) != list(range(1, len(ns) + 1)) for ns in sequences.values()):
        raise ValueError("Template indexed sequence has a gap or duplicate")
    return Files(commit, raw[INDEX], tuple(content))
