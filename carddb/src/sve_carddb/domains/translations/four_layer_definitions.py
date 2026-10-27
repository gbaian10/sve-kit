"""Content-checked shared storage expands before the frame identity boundary."""

import re
from copy import deepcopy
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, field_validator

from sve_carddb.contracts.four_layer import Hash, hash_payload
from sve_carddb.core.authored import check_path, read, require_directory
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.core.models import RecordData, UInt

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

AREA = "translations/semantic-definitions/"
INDEX = AREA + "index.yaml"
_SEQUENCE = re.compile(r"[0-9]{3,}\.yaml\Z")
_REFERENCE = re.compile(r"@([1-9][0-9]*)\Z")
_MAX_DEPTH = 64
type Files = tuple[tuple[str, bytes, bytes], ...]


class Entry(RecordData):
    id: UInt
    hash: Hash
    value: JsonValue

    @field_validator("id")
    @classmethod
    def _positive(cls, value: int) -> int:
        if not value:
            raise ValueError("Definition storage ID must be positive")
        return value


class _Envelope(RecordData):
    format: Literal[1]

    @field_validator("format", mode="before")
    @classmethod
    def _integer(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Definition storage format must be integer one")
        return value


class DefinitionIndex(_Envelope):
    kind: Literal["frame_semantics_index"]
    includes: tuple[str, ...]


class DefinitionShard(_Envelope):
    kind: Literal["frame_semantics_shard"]
    definitions: tuple[Entry, ...]


class Definitions:
    def __init__(self, entries: Sequence[Entry]) -> None:
        self.entries = tuple(entries)
        self._stored = {entry.id: entry for entry in entries}
        self._values: dict[int, JsonValue] = {}
        self._visiting: set[int] = set()
        if len(self._stored) != len(entries) or len({e.hash for e in entries}) != len(
            entries
        ):
            raise ValueError("Duplicate shared definition ID or content hash")
        for entry in entries:
            self.get(entry.id)

    def get(self, identifier: int) -> JsonValue:
        """Return detached content so consumers cannot mutate another frame through sharing."""
        if type(identifier) is not int:
            raise ValueError("Definition storage ID must be an integer")
        if identifier not in self._values:
            if identifier not in self._stored:
                raise ValueError("Missing shared semantic definition")
            if identifier in self._visiting or len(self._visiting) >= _MAX_DEPTH:
                raise ValueError("Cyclic or excessively deep semantic definition")
            self._visiting.add(identifier)
            entry = self._stored[identifier]
            value = self._expand(entry.value)
            if hash_payload(value) != entry.hash:
                raise ValueError("Shared semantic definition hash mismatch")
            self._values[identifier] = value
            self._visiting.remove(identifier)
        return deepcopy(self._values[identifier])

    def _expand(self, value: JsonValue) -> JsonValue:
        if isinstance(value, str):
            if value.startswith("@@"):
                return value[1:]
            if value.startswith("@"):
                match = _REFERENCE.fullmatch(value)
                if match is None:
                    raise ValueError("Invalid shared semantic reference")
                return self.get(int(match[1]))
        elif isinstance(value, list):
            return [self._expand(item) for item in value]
        elif isinstance(value, dict):
            expanded: dict[str, JsonValue] = {}
            for key, item in value.items():
                name = self._expand(key)
                if not isinstance(name, str) or name in expanded:
                    raise ValueError("Invalid or duplicate expanded definition key")
                expanded[name] = self._expand(item)
            return expanded
        return value

    def expand_shard(self, content: bytes) -> bytes:
        """Only the storage boundary accepts handles; downstream models retain full content."""
        document = object_value(parse(content))
        records = document.get("records")
        if isinstance(records, list):
            for record in records:
                if (
                    isinstance(record, dict)
                    and record.get("kind") == "sentence_template"
                ):
                    source = object_value(
                        object_value(record.get("data")).get("source")
                    )
                    reference = source.get("semantic_definitions")
                    if type(reference) is not int:
                        raise ValueError(
                            "Authored frame requires a semantic definition reference"
                        )
                    source["semantic_definitions"] = object_value(self.get(reference))
        return canonical(document)


def from_files(files: Files) -> Definitions:
    """Reject missing, hidden or duplicate members before exposing any definitions."""
    selected = {path: content for path, _, content in files if path.startswith(AREA)}
    if len(selected) != sum(path.startswith(AREA) for path, _, _ in files):
        raise ValueError("Duplicate semantic definition file")
    if not selected:
        return Definitions(())
    if INDEX not in selected:
        raise ValueError("Missing semantic definition index")
    index = DefinitionIndex.model_validate_json(canonical(parse(selected[INDEX])))
    if len(set(index.includes)) != len(index.includes) or any(
        _SEQUENCE.fullmatch(name) is None for name in index.includes
    ):
        raise ValueError("Invalid semantic definition index paths")
    if set(selected) != {INDEX, *(AREA + name for name in index.includes)}:
        raise ValueError(
            "Semantic definition index does not match its complete file set"
        )
    entries = tuple(
        entry
        for name in index.includes
        for entry in DefinitionShard.model_validate_json(
            canonical(parse(selected[AREA + name]))
        ).definitions
    )
    return Definitions(entries)


def read_files(root: Path) -> Files:
    """Read the entire dedicated area rather than trusting an index to hide files."""
    directory = root / AREA
    check_path(root, directory)
    if not directory.exists():
        return ()
    require_directory(root, directory)
    result = []
    for path in sorted(directory.iterdir()):
        if path.name != "index.yaml" and _SEQUENCE.fullmatch(path.name) is None:
            raise ValueError("Unexpected semantic definition file")
        raw, content = read(path, root=root)
        result.append((path.relative_to(root).as_posix(), raw, content))
    return tuple(result)
