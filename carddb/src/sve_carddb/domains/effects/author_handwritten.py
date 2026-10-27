"""Decode author shorthand and resolve only explicit source positions."""

from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING, Annotated, Literal

from pydantic import BeforeValidator, Field, JsonValue, TypeAdapter, ValidationError

from sve_carddb.contracts.author_leaves import ParamDecl, ParamSource, ParamType
from sve_carddb.contracts.four_layer import FaceRevisionOwner
from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical, integer
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.effects.author_expansion import body_version
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    ParsedDocument,
    parse_document,
)
from sve_carddb.domains.effects.author_sites import NodeSites, site_rule
from sve_carddb.domains.effects.author_sources import (
    SourceContext,
    SourceError,
    card_keyword_values,
    entry_heads,
    parameter_expected,
)

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_current import CurrentCard
    from sve_carddb.domains.translations.four_layer_classification import Classifier

Positive = Annotated[int, Field(gt=0)]
Version = Annotated[Literal[2], BeforeValidator(integer)]


class ConfigurationError(ValueError):
    """Distinguish command configuration from rejected authored data."""


class MainFile(RecordData):
    format: Version
    kind: Literal["author_effect_set"]
    scope: Literal["shared"] = "shared"
    dsl: Literal["1.0"] = "1.0"
    cards: dict[Text, dict[str, JsonValue]]


class Parameter(RecordData):
    type: ParamType
    source: ParamSource | None = None


class Body(RecordData):
    id: Text
    admission: Literal["light", "shared"] | None = None
    body: dict[str, JsonValue]
    written_by: Annotated[list[JsonValue], Field(min_length=1)]
    params: dict[str, ParamType | Parameter] = Field(default_factory=dict)
    ability_scope: str = "ability_0"
    edition: Positive = 1
    ports: dict[str, JsonValue] = Field(
        default_factory=lambda: dict[str, JsonValue](imports={}, exports={})
    )
    registrations: list[JsonValue] = Field(
        default_factory=lambda: list[JsonValue](
            [{"id": "jp", "domain": {"regions": ["jp"]}, "reviews": []}]
        )
    )
    note: Text | None = None


class BodyFile(RecordData):
    format: Version
    kind: Literal["effect_body_shard"]
    scope: Literal["shared"] = "shared"
    dsl: Literal["1.0"] = "1.0"
    records: list[Body]


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


def read_files(path: Path) -> tuple[tuple[Path, ParsedDocument], ...]:
    """Read one input or numeric owner shards; reject links at every level."""
    check_path(Path(path.anchor), path)
    if path.is_file():
        with path.open("rb") as stream:
            return ((path, parse_document(stream.read(MAX_BYTES))),)
    if not path.is_dir():
        raise FileNotFoundError(path)
    files: list[tuple[Path, ParsedDocument]] = []
    for entry in sorted(path.iterdir()):
        check_path(path, entry)
        if entry.is_dir():
            files.extend(read_files(entry))
        elif entry.suffix == ".yaml" and entry.stem.isdecimal():
            files.append((entry, parse_document(entry.read_bytes())))
        else:
            raise AuthorInputError(Diagnostic("yaml", "invalid_input_path"))
    return tuple(files)


def read(main: Path, bodies: Path | None) -> Handwritten:
    """Load the complete supplied name table; duplicate editions never merge."""
    cards: dict[str, dict[str, JsonValue]] = {}
    documents: dict[str, ParsedDocument] = {}
    definitions: dict[tuple[str, int], Body] = {}
    for path, document in read_files(main):
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


@dataclass(frozen=True)
class Position:
    face: int
    face_name: str
    face_id: str
    section: int | str
    line: int
    scope: int | None
    source: SourceContext
    ability_scope: str | None
    supported: bool
    pending_causes: tuple[str, ...] = ()
    gap: str | None = None

    def report(self) -> dict[str, JsonValue]:
        """Expose exact identity without claiming a current author proof."""
        return {
            "face": self.face,
            "face_name": self.face_name,
            "face_id": self.face_id,
            "section": self.section,
            "line": self.line,
            "scope": self.scope,
            "ability_scope": self.ability_scope,
            "frame_id": self.source.frame.id,
            "binding_id": self.source.binding.id,
            "occurrence": self.source.binding.occurrence_key().model_dump(mode="json"),
            "sentence": self.source.frame.semantic_variant.key,
            "state": self.source.frame.semantic_variant.state,
            "supported": self.supported,
            "pending_causes": list[JsonValue](self.pending_causes),
            "gap": self.gap,
            "values": self.source.binding.model_dump(mode="json")["values"],
            "slots": [
                slot.model_dump(mode="json")
                for slot in self.source.frame.leaf_schema.slots
            ],
        }


def positions(card: CurrentCard, classifier: Classifier) -> tuple[Position, ...]:  # ruff: ignore[complex-structure, too-many-locals] -- position grouping preserves source order and root identity together
    """Retain pending locations and order roots only within their source position."""
    result: list[Position] = []
    seen: set[tuple[str, str | None]] = set()
    for field in card.fields:
        for index, part in enumerate(field.field.parts):
            frame, binding = field.derived.bind(index)
            if frame.role != "body":
                continue
            owner = binding.source.owner
            if not isinstance(owner, FaceRevisionOwner):
                raise SourceError("unsupported_source_owner")
            face_id = card.revisions[owner.revision_id]
            context = SourceContext(frame, binding, part.canonical_source, classifier)
            gap = None
            try:
                heads = entry_heads(frame)
                roots: tuple[str | None, ...] = tuple(
                    f"ability_{i}" for i in range(len(heads))
                )
                supported = True
            except SourceError as error:
                roots = (None,)
                supported = False
                gap = error.reason
                if frame.semantic_variant.key == "card_keywords.v2":
                    try:
                        card_keyword_values(context)
                        supported = True
                        gap = None
                    except SourceError as error:
                        gap = error.reason
            for root in roots:
                identity = binding.id, root
                if identity in seen:
                    continue
                seen.add(identity)
                number, side = card.face_positions[face_id]
                result.append(
                    Position(
                        number,
                        side,
                        face_id,
                        "main"
                        if binding.source.field == "effect"
                        else (binding.source.ordinal or 0) + 1,
                        binding.occurrence_key().line_ordinal + 1,
                        None,
                        context,
                        root,
                        supported and field.matches[index] is not None,
                        field.derived.pending_causes[index],
                        gap
                        or (
                            "source_registry_mismatch"
                            if field.matches[index] is None
                            else None
                        ),
                    )
                )
    groups: dict[tuple[str, int | str, int], list[Position]] = {}
    for position in result:
        groups.setdefault(
            (position.face_id, position.section, position.line), []
        ).append(position)
    ordered: list[Position] = []
    for group in groups.values():
        abilities = [
            p
            for p in group
            if p.source.frame.semantic_variant.key != "card_keywords.v2"
        ]
        abilities.sort(
            key=lambda p: (
                tuple(
                    (s.start, s.end) for s in p.source.binding.occurrence_key().segments
                ),
                int((p.ability_scope or "ability_0").split("_")[-1]),
            )
        )
        keys = [
            (p.source.binding.occurrence_key().segments, p.ability_scope)
            for p in abilities
        ]
        if any(keys.count(key) > 1 for key in keys):
            raise SourceError("ambiguous_source_classification")
        ordered.extend(
            replace(p, scope=abilities.index(p) + 1) if p in abilities else p
            for p in group
        )
    return tuple(ordered)


def resolve(  # ruff: ignore[complex-structure] -- keep the complete ordered boundary in one operation
    candidates: tuple[Position, ...], locator: Locator, *, keywords: bool
) -> Position:
    """Resolve a unique location, without looking at a body name, value or head."""
    selected = [
        p
        for p in candidates
        if p.line == locator.line
        and (p.source.frame.semantic_variant.key == "card_keywords.v2") == keywords
    ]
    for key in ("face", "section", "scope"):
        if key == "scope" and len(selected) > 1:
            for earlier in ("face", "section"):
                options = list(dict.fromkeys(getattr(p, earlier) for p in selected))
                if len(options) > 1:
                    raise AuthorInputError(
                        Diagnostic(
                            "source",
                            "ambiguous_source",
                            expected=f"{earlier}: "
                            + ", ".join(str(option) for option in options),
                        )
                    )
        value = getattr(locator, key)
        if value is not None:
            selected = [
                p
                for p in selected
                if getattr(p, key) == value or (key == "face" and p.face_name == value)
            ]
    if not selected:
        raise AuthorInputError(Diagnostic("source", "missing_source"))
    if len(selected) > 1:
        for key in ("face", "section", "scope"):
            values = list(dict.fromkeys(getattr(p, key) for p in selected))
            if len(values) > 1:
                hint = ", ".join(str(value) for value in values)
                raise AuthorInputError(
                    Diagnostic("source", "ambiguous_source", expected=f"{key}: {hint}")
                )
        raise AuthorInputError(Diagnostic("source", "ambiguous_source_classification"))
    position = selected[0]
    if not position.supported or (not keywords and position.ability_scope is None):
        raise AuthorInputError(Diagnostic("source", "unsupported_source_projection"))
    return position


def materialize(
    body: Body, source: SourceContext, sites: NodeSites
) -> dict[str, JsonValue]:
    """Infer parameters from typed syntax and source roles, never scalar equality."""
    declarations: dict[str, JsonValue] = {}
    parameters = sites.parameters(body.body)
    if {p.name for p in parameters} != body.params.keys():
        raise SourceError("parameter_name_mismatch")
    for name, value in body.params.items():
        parameter = Parameter(type=value) if isinstance(value, str) else value
        used = [p for p in parameters if p.name == name]
        options: list[ParamSource] = []
        for slot in source.frame.leaf_schema.slots:
            if all(
                (rule := site_rule(site)).type == parameter.type
                and rule.source_type == slot.type
                and rule.role == slot.role
                for site in used
            ):
                components = {site_rule(site).component for site in used}
                if len(components) == 1:
                    options.append(
                        ParamSource.model_validate(
                            {"slot": slot.name, "component": next(iter(components))}
                        )
                    )
        if parameter.source is not None:
            options = [option for option in options if option == parameter.source]
        if len(options) != 1:
            raise SourceError(
                "ambiguous_parameter_source" if options else "missing_parameter_source"
            )
        declaration = ParamDecl(type=parameter.type, source=options[0])
        for site in used:
            parameter_expected(
                source, declaration, site.owner, site.relative_path, site.fragment
            )
        declarations[name] = declaration.model_dump(mode="json")
    record: dict[str, JsonValue] = {
        "frame_id": source.frame.id,
        "ability_scope": body.ability_scope,
        "dsl": "1.0",
        "params": declarations,
        "body": body.body,
        "ports": body.ports,
        "written_by": body.written_by,
        "registrations": body.registrations,
    }
    if body.note is not None:
        record["note"] = body.note
    record["body_version"] = body_version(record)
    return record
