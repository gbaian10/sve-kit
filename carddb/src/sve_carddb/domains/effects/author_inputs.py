"""Validate author structure and local invariants, without admitting execution."""

import re
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Literal, get_args

import yamlrocks
from jsonschema import Draft202012Validator
from jsonschema.exceptions import best_match
from pydantic import JsonValue, TypeAdapter, ValidationError
from referencing import Registry, Resource
from referencing.exceptions import Unresolvable

from sve_carddb.contracts.author_entries import (
    Ability,
    AuthorEffectSet,
    EffectBodyShard,
)
from sve_carddb.contracts.author_evidence import (
    CardKeywordSourceUse,
    DirectSourceUse,
    SourceUse,
)
from sve_carddb.contracts.author_leaves import AuthorIndexRecord, EffectsIndex
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, YamlSyntaxError, parse_yaml
from sve_carddb.domains.effects.author_validation import (
    expanded_residuals,
    inline_parameters,
)

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from jsonschema.exceptions import ValidationError as SchemaError
    from referencing.jsonschema import Schema

PathParts = tuple[str | int, ...]
Leaf = Literal["A.param", "ParamDecl", "ParamValue", "ParamDeclarations", "ParamValues"]
_NODES = "urn:sve-kit:author-nodes:1.0"
_INDEX = "urn:sve-kit:effects-index:1"
_AUTHOR = "urn:sve-kit:author:1.0"
_BODIES = "urn:sve-kit:effect-bodies:1.0"
_PROTOTYPE = "urn:sve-kit:effects:astra:1"
_RESOURCES = {
    "author-nodes.schema.json": _NODES,
    "effects-index.schema.json": _INDEX,
    "author.schema.json": _AUTHOR,
    "effect-bodies.schema.json": _BODIES,
    "effects.schema.json": _PROTOTYPE,
    "author-storage.schema.json": "urn:sve-kit:author-storage:2",
    "effect-body-storage.schema.json": "urn:sve-kit:effect-body-storage:2",
}
_INDEX_VALUE = TypeAdapter(EffectsIndex)
_AUTHOR_VALUE = TypeAdapter(AuthorEffectSet)
_BODIES_VALUE = TypeAdapter(EffectBodyShard)
type AuthorDocument = AuthorEffectSet | EffectBodyShard | EffectsIndex
type Stage = Literal[
    "yaml",
    "json",
    "schema",
    "index",
    "envelope",
    "container",
    "source",
    "binding",
    "frame",
    "body",
    "params",
    "recipe",
    "dependency",
    "anchor",
]


@dataclass(frozen=True)
class Diagnostic:
    stage: Stage
    reason: str
    path: PathParts = ()
    line: int | None = None
    column: int | None = None
    expected: str | None = None
    actual: str | None = None
    card_id: str | None = None
    card_no: str | None = None
    scope: str | None = None
    source: SourceUse | CardKeywordSourceUse | DirectSourceUse | None = None
    card_line: int | None = None


class AuthorInputError(ValueError):
    """Only structural locations and categories may leave the input boundary."""

    def __init__(self, diagnostic: Diagnostic) -> None:
        self.diagnostic = diagnostic
        super().__init__(
            f"Author input rejected: {diagnostic.stage}/{diagnostic.reason}"
        )


@dataclass(frozen=True)
class _Locations:
    locations: yamlrocks.YAMLRocksDocument

    def diagnostic(
        self,
        stage: Stage,
        reason: str,
        path: PathParts,
        expected: str | None = None,
        actual: str | None = None,
    ) -> Diagnostic:
        # Merged keys have no direct cursor; report the nearest containing mapping.
        lookup = path
        node = self.locations.locate(lookup)
        while node is None and lookup:
            lookup = lookup[:-1]
            node = self.locations.locate(lookup)
        line, column = (node.line, node.column) if node else (None, None)
        return Diagnostic(stage, reason, path, line, column, expected, actual)


@dataclass(frozen=True)
class ParsedDocument(_Locations):
    value: JsonValue
    context: AuthorIndexRecord | None = None


def _contextual(
    diagnostic: Diagnostic, context: AuthorIndexRecord | None
) -> Diagnostic:
    if context is None or diagnostic.card_no not in {None, context.record_key}:
        return diagnostic
    return replace(
        diagnostic,
        card_id=context.card_id,
        card_no=diagnostic.card_no or context.record_key,
        scope=diagnostic.scope or context.scope,
    )


def _source_at(
    value: JsonValue, path: PathParts
) -> SourceUse | CardKeywordSourceUse | None:
    prefixes = (
        ("cards", str, "abilities", int),
        ("cards", str, "faces", str, "abilities", int),
    )
    length = next(
        (
            len(prefix)
            for prefix in prefixes
            if len(path) >= len(prefix)
            and all(
                isinstance(part, expected)
                if isinstance(expected, type)
                else part == expected
                for part, expected in zip(path, prefix, strict=False)
            )
        ),
        None,
    )
    if length is None:
        return None
    node = value
    for part in path[:length]:
        if isinstance(node, dict) and isinstance(part, str):
            node = node.get(part)
        elif isinstance(node, list) and isinstance(part, int) and 0 <= part < len(node):
            node = node[part]
        else:
            return None
    if isinstance(node, dict):
        sources = node.get("sources")
        if not isinstance(sources, list) or len(sources) != 1:
            return None
        try:
            source_type = CardKeywordSourceUse if "keywords" in node else SourceUse
            return source_type.model_validate_json(canonical(sources[0]), strict=True)
        except ValidationError:
            return None
    return None


def _located(document: ParsedDocument, diagnostic: Diagnostic) -> Diagnostic:
    value = document.value
    if isinstance(value, dict):
        scope = value.get("scope")
        card_no = None
        if len(diagnostic.path) > 1 and diagnostic.path[0] == "cards":
            key = diagnostic.path[1]
            if isinstance(key, str):
                card_no = key
        diagnostic = replace(
            diagnostic,
            card_no=card_no,
            scope=scope if isinstance(scope, str) else None,
            source=_source_at(value, diagnostic.path),
        )
    return _contextual(diagnostic, document.context)


def _references(value: JsonValue) -> list[str]:
    result: list[str] = []
    pending = [value]
    while pending:
        node = pending.pop()
        if isinstance(node, dict):
            for keyword in ("$ref", "$dynamicRef"):
                reference = node.get(keyword)
                if isinstance(reference, str):
                    result.append(reference)
            pending.extend(node.values())
        elif isinstance(node, list):
            pending.extend(node)
    return result


def _entry_decode[T](document: ParsedDocument, adapter: TypeAdapter[T]) -> T:
    try:
        return adapter.validate_json(canonical(document.value), strict=True)
    except ValidationError as error:
        detail = error.errors(include_input=False, include_context=False)[0]
        node = document.value
        path: PathParts = ()
        # Pydantic union labels are not wire keys and must not leak into cursors.
        for part in detail["loc"]:
            if isinstance(node, dict) and isinstance(part, str) and part in node:
                node = node[part]
                path = (*path, part)
                continue
            if (
                isinstance(node, list)
                and isinstance(part, int)
                and 0 <= part < len(node)
            ):
                node = node[part]
                path = (*path, part)
        raise AuthorInputError(
            _located(
                document, document.diagnostic("schema", "decoder_constraint", path)
            )
        ) from None


def _envelope(document: ParsedDocument, expected: str) -> dict[str, JsonValue]:
    value = document.value
    if not isinstance(value, dict):
        raise AuthorInputError(
            _located(document, document.diagnostic("envelope", "expected_mapping", ()))
        )
    if type(value.get("format")) is not int or value["format"] != 1:
        raise AuthorInputError(
            _located(
                document,
                document.diagnostic("envelope", "unsupported_format", ("format",)),
            )
        )
    if value.get("kind") != expected:
        raise AuthorInputError(
            _located(
                document, document.diagnostic("envelope", "unsupported_kind", ("kind",))
            )
        )
    return value


def _entry_preflight(document: ParsedDocument, expected: str) -> None:
    value = _envelope(document, expected)
    if expected == "author_effect_set" and value.get("scope") == "en_override":
        raise AuthorInputError(
            _located(
                document,
                document.diagnostic(
                    "source", "en_source_contract_unavailable", ("scope",)
                ),
            )
        )
    candidates: list[tuple[PathParts, JsonValue]] = []
    cards = value.get("cards")
    if expected == "author_effect_set" and isinstance(cards, dict):
        candidates = [
            (("cards", key, "meta"), card.get("meta"))
            for key, card in cards.items()
            if isinstance(card, dict)
        ]
    records = value.get("records")
    if expected == "effect_body_shard" and isinstance(records, list):
        candidates = [
            (("records", index), record) for index, record in enumerate(records)
        ]
    _candidate_checks(document, candidates)


def _candidate_checks(
    document: ParsedDocument, candidates: list[tuple[PathParts, JsonValue]]
) -> None:
    for path, candidate in candidates:
        if not isinstance(candidate, dict):
            continue
        if "dsl" in candidate and candidate["dsl"] != "1.0":
            raise AuthorInputError(
                _located(
                    document,
                    document.diagnostic("envelope", "unsupported_dsl", (*path, "dsl")),
                )
            )
        exams = candidate.get("verified_by_exam")
        if isinstance(exams, list) and exams:
            raise AuthorInputError(
                _located(
                    document,
                    document.diagnostic(
                        "container",
                        "unsupported_exam_evidence",
                        (*path, "verified_by_exam"),
                    ),
                )
            )


def _abilities(decoded: AuthorEffectSet) -> Iterator[tuple[PathParts, Ability]]:
    for key, card in decoded.cards.items():
        for index, ability in enumerate(card.get("abilities", ())):
            yield ("cards", key, "abilities", index), ability
        for face_id, face in card.get("faces", {}).items():
            for index, ability in enumerate(face.abilities):
                yield ("cards", key, "faces", face_id, "abilities", index), ability


def _body_duplicates(document: ParsedDocument, decoded: EffectBodyShard) -> None:
    keys: set[tuple[str, str, str]] = set()
    for offset, record in enumerate(decoded.records):
        key = (record["frame_id"], record["ability_scope"], record["body_version"])
        if key in keys:
            raise AuthorInputError(
                document.diagnostic("container", "duplicate_body", ("records", offset))
            )
        keys.add(key)
        identifiers: set[str] = set()
        for index, registration in enumerate(record["registrations"]):
            if registration.id in identifiers:
                raise AuthorInputError(
                    document.diagnostic(
                        "container",
                        "duplicate_registration",
                        ("records", offset, "registrations", index, "id"),
                    )
                )
            identifiers.add(registration.id)


def _local_invariants(
    document: ParsedDocument, decoded: AuthorEffectSet | EffectBodyShard
) -> None:
    if isinstance(decoded, EffectBodyShard):
        _body_duplicates(document, decoded)
        return
    for path, ability in _abilities(decoded):
        node = ability.root
        if "keywords" in node:
            key = str(path[1])
            metadata = decoded.cards[key]["meta"]
            if metadata.status != "draft" or metadata.reviewed_by:
                raise AuthorInputError(
                    _located(
                        document,
                        document.diagnostic(
                            "container",
                            "unsupported_keyword_review_evidence",
                            ("cards", key, "meta"),
                        ),
                    )
                )
        if "generated_by" in node:
            continue
        for violation in inline_parameters(node, path):
            diagnostic = _located(
                document,
                document.diagnostic("container", violation.reason, violation.path),
            )
            raise AuthorInputError(replace(diagnostic, source=node["sources"][0]))


def _json_value(
    value: object, locations: _Locations, path: PathParts = ()
) -> JsonValue:
    if isinstance(value, dict):
        result: dict[str, JsonValue] = {}
        for key, child in value.items():
            if not isinstance(key, str):
                # Non-string keys cannot form JSON paths or round-trip cursors.
                raise AuthorInputError(
                    locations.diagnostic("json", "non_string_key", path)
                )
            _json_value(key, locations, (*path, key))
            result[key] = _json_value(child, locations, (*path, key))
        return result
    if isinstance(value, list):
        return [
            _json_value(child, locations, (*path, index))
            for index, child in enumerate(value)
        ]
    try:
        scalar = JSON_VALUE.validate_python(value, strict=True)
    except ValidationError:
        raise AuthorInputError(
            locations.diagnostic("json", "non_json_value", path)
        ) from None
    try:
        canonical(scalar)
    except ValueError, TypeError:
        raise AuthorInputError(
            locations.diagnostic("json", "noncanonical_value", path)
        ) from None
    return scalar


def parse_document(raw: bytes) -> ParsedDocument:
    """Decode strict, bounded JSON-compatible YAML with safe source coordinates."""
    if len(raw) >= MAX_BYTES:
        raise AuthorInputError(Diagnostic("yaml", "size_limit"))
    try:
        parsed = parse_yaml(raw)
    except YamlSyntaxError as error:
        raise AuthorInputError(
            Diagnostic("yaml", error.reason, line=error.line, column=error.column)
        ) from None
    except ValueError:
        raise AuthorInputError(Diagnostic("yaml", "invalid_document")) from None
    # Round-trip decoding supplies locations; only the strict parser supplies values.
    locations = yamlrocks.loads(raw, option=yamlrocks.OPT_ROUND_TRIP)
    if not isinstance(locations, yamlrocks.YAMLRocksDocument):
        raise AuthorInputError(Diagnostic("yaml", "missing_document"))
    return ParsedDocument(locations, _json_value(parsed, _Locations(locations)))


def _document(raw: bytes, context: AuthorIndexRecord | None = None) -> ParsedDocument:
    try:
        return replace(parse_document(raw), context=context)
    except AuthorInputError as error:
        raise AuthorInputError(_contextual(error.diagnostic, context)) from None


def _schema_diagnostic(document: ParsedDocument, error: SchemaError) -> Diagnostic:
    if error.validator in {"oneOf", "anyOf"}:
        mismatched = {
            child.schema_path[0]
            for child in error.context
            if child.validator == "const"
        }
        candidates = [
            child for child in error.context if child.schema_path[0] not in mismatched
        ]
        # A branch missing its own required key here is another variant, so prefer
        # the single branch whose keys are present and report its deeper error.
        absent = {
            child.schema_path[0]
            for child in candidates
            if child.validator == "required" and not child.relative_path
        }
        present = [child for child in candidates if child.schema_path[0] not in absent]
        if len({child.schema_path[0] for child in present}) == 1:
            candidates = present
        if len({child.schema_path[0] for child in candidates}) == 1:
            selected = best_match(candidates)
            if selected is not None:
                return _schema_diagnostic(document, selected)
    expected = None
    actual = None
    if error.validator == "type":
        expected = str(error.validator_value)
        actual = type(error.instance).__name__
    path = tuple(error.absolute_path)
    if (
        error.validator == "additionalProperties"
        and isinstance(error.instance, dict)
        and isinstance(error.schema, dict)
    ):
        unknown = [
            key
            for key in error.instance
            if key not in error.schema.get("properties", {})
            and not any(
                re.search(pattern, key)
                for pattern in error.schema.get("patternProperties", {})
            )
        ]
        if unknown:
            path = (*path, unknown[0])
    elif "propertyNames" in error.absolute_schema_path and isinstance(
        error.instance, str
    ):
        path = (*path, error.instance)
    return document.diagnostic("schema", str(error.validator), path, expected, actual)


def _decode[T](adapter: TypeAdapter[T], value: JsonValue) -> T:
    try:
        return adapter.validate_json(canonical(value), strict=True)
    except ValidationError:
        # Drift must not expose pydantic's input_value excerpts to callers.
        raise AuthorInputError(Diagnostic("schema", "decoder_mismatch")) from None


@dataclass(frozen=True)
class _HeadShape:
    schema: dict[str, JsonValue]
    heads: frozenset[str]
    keys: frozenset[str]
    branches: dict[str, int] | None = None

    def diagnostic(
        self, document: ParsedDocument, error: SchemaError
    ) -> Diagnostic | None:
        value = error.instance
        if error.schema != self.schema or not isinstance(value, dict):
            return None
        present = self.heads.intersection(value)
        path = tuple(error.absolute_path)
        if len(present) > 1:
            return document.diagnostic("container", "ability_head_conflict", path)
        unknown = next((key for key in value if key not in self.keys), None)
        if unknown is not None:
            return document.diagnostic(
                "schema", "additionalProperties", (*path, unknown)
            )
        if not present:
            return document.diagnostic("container", "ability_head_missing", path)
        return None

    def selected_error(self, error: SchemaError) -> SchemaError | None:
        if (
            error.schema != self.schema
            or not isinstance(error.instance, dict)
            or self.branches is None
        ):
            return None
        present = self.heads.intersection(error.instance)
        if len(present) != 1:
            return None
        branch = self.branches[next(iter(present))]
        selected: SchemaError | None = best_match(
            child for child in error.context if child.schema_path[0] == branch
        )
        return selected


def _head_shapes(registry: Registry[Schema]) -> tuple[_HeadShape, ...]:
    resolver = registry.resolver(_NODES)

    def schema(reference: str) -> dict[str, JsonValue]:
        return object_value(
            JSON_VALUE.validate_python(resolver.lookup(reference).contents)
        )

    def required(value: dict[str, JsonValue]) -> frozenset[str]:
        names = value["required"]
        assert isinstance(names, list)
        assert all(isinstance(name, str) for name in names)
        return frozenset(name for name in names if isinstance(name, str))

    inline = schema("#/$defs/A.inline-ability")
    choices = inline["anyOf"]
    assert isinstance(choices, list)
    variants = []
    for choice in choices:
        reference = object_value(choice)["$ref"]
        assert isinstance(reference, str)
        variants.append(schema(reference))
    requirements = [required(item) for item in variants]
    common = frozenset.intersection(*requirements)
    heads = frozenset.union(*requirements) - common
    keys = frozenset(
        key for item in variants for key in object_value(item["properties"])
    )
    ability = schema("#/$defs/A.ability")
    choices = ability["anyOf"]
    assert isinstance(choices, list)
    # Resolve the authored anchor, never the expanded profile's false override.
    generated_index, generated = next(
        (index, schema(reference))
        for index, choice in enumerate(choices)
        if isinstance(reference := object_value(choice).get("$dynamicRef"), str)
    )
    inline_index = next(
        index
        for index, choice in enumerate(choices)
        if isinstance(reference := object_value(choice).get("$ref"), str)
        and schema(reference) == inline
    )
    generated_heads = required(generated) - common
    shapes = [
        _HeadShape(inline, heads, keys),
        _HeadShape(
            ability,
            heads | generated_heads,
            keys | object_value(generated["properties"]).keys(),
            {
                **dict.fromkeys(heads, inline_index),
                **dict.fromkeys(generated_heads, generated_index),
            },
        ),
    ]
    for reference in ("#/$defs/A.ability-head", "#/$defs/A.granted-ability-head"):
        head = schema(reference)
        keys = frozenset(object_value(head["properties"]))
        shapes.append(_HeadShape(head, keys, keys))
    return tuple(shapes)


class AuthorSchemas:
    """Load the fixed local schema resources; never resolve remote references."""

    def __init__(self, schema_directory: Path) -> None:
        registry: Registry[Schema] = Registry()
        schemas: dict[str, dict[str, JsonValue]] = {}
        for filename, identity in _RESOURCES.items():
            schema = object_value(parse((schema_directory / filename).read_bytes()))
            if schema.get("$id") != identity:
                raise ValueError("Unexpected author schema identity")
            Draft202012Validator.check_schema(schema)
            registry = registry.with_resource(identity, Resource.from_contents(schema))
            schemas[identity] = schema
        for identity, schema in schemas.items():
            for reference in _references(schema):
                try:
                    registry.resolver(identity).lookup(reference)
                except Unresolvable:
                    raise ValueError("Unresolved author schema reference") from None
        self._registry = registry
        self._heads = _head_shapes(registry)
        envelope = object_value(
            JSON_VALUE.validate_python(
                registry.resolver(_NODES).lookup("#/$defs/A.file").contents
            )
        )
        properties = object_value(envelope["properties"])
        self._inventory = {
            **envelope,
            "properties": {
                **properties,
                "cards": {
                    **object_value(properties["cards"]),
                    "additionalProperties": True,
                },
            },
        }

    def _validate(self, document: ParsedDocument, reference: str) -> None:
        self._validate_schema(document, {"$ref": reference})

    def _validate_schema(self, document: ParsedDocument, schema: Schema) -> None:
        validator = Draft202012Validator(schema, registry=self._registry)
        error = best_match(validator.iter_errors(document.value))
        if error is not None:
            ancestors = []
            current = error
            while current is not None:
                ancestors.append(current)
                current = current.parent
            for ancestor in reversed(ancestors):
                for shape in self._heads:
                    if diagnostic := shape.diagnostic(document, ancestor):
                        raise AuthorInputError(_located(document, diagnostic))
            for ancestor in reversed(ancestors):
                for shape in self._heads:
                    if selected := shape.selected_error(ancestor):
                        raise AuthorInputError(
                            _located(document, _schema_diagnostic(document, selected))
                        )
            raise AuthorInputError(
                _located(document, _schema_diagnostic(document, error))
            )

    def storage_tree(self, raw: bytes, kind: str) -> JsonValue:
        """Validate persisted handwritten data, retaining defaults absent."""
        references = {
            "author_effect_set": "urn:sve-kit:author-storage:2",
            "effect_body_shard": "urn:sve-kit:effect-body-storage:2",
        }
        document = _document(raw)
        self._validate(document, references[kind])
        return document.value

    def _author(
        self, document: ParsedDocument, *, expanded: bool = False
    ) -> AuthorEffectSet:
        _entry_preflight(document, "author_effect_set")
        reference = _NODES + "#/$defs/expanded" if expanded else _AUTHOR
        self._validate(document, reference)
        decoded = _entry_decode(document, _AUTHOR_VALUE)
        if expanded:
            for violation in expanded_residuals(decoded):
                raise AuthorInputError(
                    _located(
                        document,
                        document.diagnostic(
                            "container", violation.reason, violation.path
                        ),
                    )
                )
        _local_invariants(document, decoded)
        return decoded

    def author_tree(
        self, raw: bytes, *, context: AuthorIndexRecord | None = None
    ) -> JsonValue:
        """Validate and retain the original JSON tree, without decoder defaults."""
        document = _document(raw, context)
        self._author(document)
        return document.value

    def author_inventory_tree(
        self, raw: bytes, *, context: AuthorIndexRecord | None = None
    ) -> JsonValue:
        """Validate shared inputs; EN values remain opaque for index closure only."""
        document = _document(raw, context)
        value = document.value
        if isinstance(value, dict) and value.get("scope") == "en_override":
            # Reuse strict format/kind checks without claiming EN body support.
            _envelope(document, "author_effect_set")
            self._validate_schema(document, self._inventory)
        else:
            self._author(document)
        return document.value

    def prototype_tree(
        self, raw: bytes, *, context: AuthorIndexRecord | None = None
    ) -> JsonValue:
        """Validate the separate prototype resource without selecting a runtime."""
        document = _document(raw, context)
        _entry_preflight(document, "effect_set")
        self._validate(document, _PROTOTYPE)
        return document.value

    def expanded(
        self, raw: bytes, *, context: AuthorIndexRecord | None = None
    ) -> AuthorEffectSet:
        """Read a fully expanded tree; this does not establish source admission."""
        return self._author(_document(raw, context), expanded=True)

    def _bodies(self, document: ParsedDocument) -> EffectBodyShard:
        _entry_preflight(document, "effect_body_shard")
        self._validate(document, _BODIES)
        decoded = _entry_decode(document, _BODIES_VALUE)
        _local_invariants(document, decoded)
        return decoded

    def body_tree(self, raw: bytes) -> JsonValue:
        """Validate companion inputs while retaining exact hash payload subtrees."""
        document = _document(raw)
        self._bodies(document)
        return document.value

    def document(
        self, raw: bytes, *, context: AuthorIndexRecord | None = None
    ) -> AuthorDocument:
        """Select exactly one registered envelope; never fall back to a prototype."""
        document = _document(raw, context)
        value = document.value
        kind = value.get("kind") if isinstance(value, dict) else None
        if kind == "author_effect_set":
            return self._author(document)
        if kind == "effect_body_shard":
            return self._bodies(document)
        if kind == "effects_index":
            return self._index(document)
        raise AuthorInputError(
            _located(
                document, document.diagnostic("envelope", "unsupported_kind", ("kind",))
            )
        )

    def leaf(self, raw: bytes, definition: Leaf) -> JsonValue:
        """Validate a fragment; this grants no placement or source-role permission."""
        if definition not in get_args(Leaf):
            raise ValueError("Unknown author leaf entry")
        document = _document(raw)
        self._validate(document, _NODES + "#/$defs/" + definition)
        return document.value

    def effects_index(self, raw: bytes) -> EffectsIndex:
        """Validate index syntax and uniqueness without opening candidate files."""
        document = _document(raw)
        return self._index(document)

    def _index(self, document: ParsedDocument) -> EffectsIndex:
        self._validate(document, _INDEX)
        index = _decode(_INDEX_VALUE, document.value)
        seen: set[tuple[str, str]] = set()
        for offset, record in enumerate(index.records):
            key = (record.card_id, record.scope)
            if key in seen:
                raise AuthorInputError(
                    document.diagnostic(
                        "index", "duplicate_card_scope", ("records", offset)
                    )
                )
            seen.add(key)
        return index
