"""Decode author shorthand and resolve only explicit source positions."""

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Annotated, Literal

from pydantic import BeforeValidator, Field, JsonValue, TypeAdapter, ValidationError

from sve_carddb.contracts.author_leaves import ParamType
from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical, integer
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    ParsedDocument,
    parse_document,
)

Positive = Annotated[int, Field(gt=0)]
Version = Annotated[Literal[2], BeforeValidator(integer)]
MAIN_SHARD_PATTERN = r"(?:[0-9]{3,}|[A-Za-z0-9_]+-[^/\\\x00-\x1f\x7f]+)\.yaml"


class ConfigurationError(ValueError):
    """Distinguish command configuration from rejected authored data."""


class MainFile(RecordData):
    format: Version
    kind: Literal["author_effect_set"]
    scope: Literal["shared"] = "shared"
    dsl: Literal["1.0"] = "1.0"
    cards: dict[Text, dict[str, JsonValue]]


class Body(RecordData):
    id: Text
    admission: Literal["light", "shared"] | None = None
    body: dict[str, JsonValue]
    written_by: Annotated[list[JsonValue], Field(min_length=1)]
    params: dict[str, ParamType] = Field(default_factory=dict)
    ability_scope: str = "ability_0"
    edition: Positive = 1
    ports: dict[str, JsonValue] = Field(
        default_factory=lambda: dict[str, JsonValue](imports={}, exports={})
    )
    note: Text | None = None


class BodyFile(RecordData):
    format: Version
    kind: Literal["effect_body_shard"]
    scope: Literal["shared"] = "shared"
    dsl: Literal["1.0"] = "1.0"
    records: list[Body]


class RuleSection(RecordData):
    section: Positive
    face: Positive | Literal["front", "back"] | None = None


class Locator(RecordData):
    line: Positive
    face: Positive | Literal["front", "back"] | None = None
    section: Positive | Literal["main"] | None = None
    scope: Positive | None = None


def decode[T](document: ParsedDocument, model: type[T]) -> T:
    """Keep pydantic values out of diagnostics, retaining the YAML cursor."""
    try:
        return TypeAdapter(model).validate_json(canonical(document.value), strict=True)
    except ValidationError as error:
        first = error.errors(include_input=False, include_url=False)[0]
        path = tuple(first["loc"])
        raise AuthorInputError(
            document.diagnostic("schema", first["type"], path)
        ) from None


@dataclass(frozen=True)
class Handwritten:
    cards: dict[str, dict[str, JsonValue]]
    bodies: dict[tuple[str, int], Body]
    documents: dict[str, ParsedDocument]


def read_files(
    path: Path, *, main: bool = False
) -> tuple[tuple[Path, ParsedDocument], ...]:
    """Read one input or owner shards; reject links at every level."""
    check_path(Path(path.anchor), path)
    if path.is_file():
        with path.open("rb") as stream:
            return ((path, parse_document(stream.read(MAX_BYTES))),)
    if not path.is_dir():
        raise FileNotFoundError(path)
    files: list[tuple[Path, ParsedDocument]] = []
    pattern = MAIN_SHARD_PATTERN if main else r"\d+\.yaml"
    for entry in sorted(path.iterdir()):
        check_path(path, entry)
        if entry.is_dir():
            files.extend(read_files(entry, main=main))
        elif re.fullmatch(pattern, entry.name) and entry.is_file():
            files.append((entry, parse_document(entry.read_bytes())))
        else:
            raise AuthorInputError(Diagnostic("yaml", "invalid_input_path"))
    return tuple(files)


def read(main: Path, bodies: Path | None) -> Handwritten:
    """Load the complete supplied name table; duplicate editions never merge."""
    cards: dict[str, dict[str, JsonValue]] = {}
    documents: dict[str, ParsedDocument] = {}
    definitions: dict[tuple[str, int], Body] = {}
    for path, document in read_files(main, main=True):
        parsed = decode(document, MainFile)
        for number, card in parsed.cards.items():
            if number in cards:
                raise AuthorInputError(
                    Diagnostic("schema", "duplicate_card", card_no=number)
                )
            if main.is_dir() and path.parent.name != number.split("-", 1)[0]:
                raise AuthorInputError(
                    Diagnostic("index", "wrong_owner", card_no=number)
                )
            cards[number] = card
            documents[number] = document
    for _, document in () if bodies is None else read_files(bodies):
        for body in decode(document, BodyFile).records:
            key = body.id, body.edition
            if key in definitions:
                raise AuthorInputError(Diagnostic("schema", "duplicate_body_id"))
            definitions[key] = body
    return Handwritten(cards, definitions, documents)
