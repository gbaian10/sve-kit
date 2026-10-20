"""Validate author leaves and the effects index, without loading executable bodies."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal, get_args

import yamlrocks
from jsonschema import Draft202012Validator
from jsonschema.exceptions import best_match
from pydantic import ConfigDict, JsonValue, TypeAdapter, ValidationError
from referencing import Registry, Resource

from sve_carddb.contracts.author_leaves import (
    EffectsIndex,
    ParamDecl,
    ParamDeclarations,
    ParamRef,
    ParamValue,
    ParamValues,
)
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, YamlSyntaxError, parse_yaml

if TYPE_CHECKING:
    from pathlib import Path

    from jsonschema.exceptions import ValidationError as SchemaError
    from referencing.jsonschema import Schema

PathParts = tuple[str | int, ...]
Leaf = Literal["A.param", "ParamDecl", "ParamValue", "ParamDeclarations", "ParamValues"]
_NODES = "urn:sve-kit:author-nodes:1.0"
_INDEX = "urn:sve-kit:effects-index:1"
_RESOURCES = {
    "author-nodes.schema.json": _NODES,
    "effects-index.schema.json": _INDEX,
}
_PARAM_REF = TypeAdapter(ParamRef)
_PARAM_DECL = TypeAdapter(ParamDecl)
_PARAM_VALUE: TypeAdapter[ParamValue] = TypeAdapter(ParamValue)
_PARAM_DECLS: TypeAdapter[ParamDeclarations] = TypeAdapter(
    ParamDeclarations, config=ConfigDict(regex_engine="python-re")
)
_PARAM_VALUES: TypeAdapter[ParamValues] = TypeAdapter(
    ParamValues, config=ConfigDict(regex_engine="python-re")
)
_INDEX_VALUE = TypeAdapter(EffectsIndex)


@dataclass(frozen=True)
class Diagnostic:
    stage: Literal["yaml", "json", "schema", "index"]
    reason: str
    path: PathParts = ()
    line: int | None = None
    column: int | None = None
    expected: str | None = None
    actual: str | None = None


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
        stage: Literal["json", "schema", "index"],
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
class _Document(_Locations):
    value: JsonValue


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


def _document(raw: bytes) -> _Document:
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
    return _Document(locations, _json_value(parsed, _Locations(locations)))


def _schema_diagnostic(document: _Document, error: SchemaError) -> Diagnostic:
    if error.validator in {"oneOf", "anyOf"}:
        mismatched = {
            child.schema_path[0]
            for child in error.context
            if child.validator == "const"
        }
        candidates = [
            child for child in error.context if child.schema_path[0] not in mismatched
        ]
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


class AuthorSchemas:
    """Load the fixed local schema resources; never resolve remote references."""

    def __init__(self, schema_directory: Path) -> None:
        registry: Registry[Schema] = Registry()
        for filename, identity in _RESOURCES.items():
            schema = object_value(parse((schema_directory / filename).read_bytes()))
            if schema.get("$id") != identity:
                raise ValueError("Unexpected author schema identity")
            Draft202012Validator.check_schema(schema)
            registry = registry.with_resource(identity, Resource.from_contents(schema))
        self._registry = registry

    def _validate(self, document: _Document, reference: str) -> None:
        validator = Draft202012Validator({"$ref": reference}, registry=self._registry)
        error = best_match(validator.iter_errors(document.value))
        if error is not None:
            raise AuthorInputError(_schema_diagnostic(document, error))

    def leaf(self, raw: bytes, definition: Leaf) -> JsonValue:
        """Validate a fragment; this grants no placement or source-role permission."""
        if definition not in get_args(Leaf):
            raise ValueError("Unknown author leaf entry")
        document = _document(raw)
        self._validate(document, _NODES + "#/$defs/" + definition)
        return document.value

    def parameter_reference(self, raw: bytes) -> ParamRef:
        """Decode a reference without substituting it into any author node."""
        return _decode(_PARAM_REF, self.leaf(raw, "A.param"))

    def parameter_declaration(self, raw: bytes) -> ParamDecl:
        """Decode a declaration; source-slot compatibility is a separate check."""
        return _decode(_PARAM_DECL, self.leaf(raw, "ParamDecl"))

    def parameter_value(self, raw: bytes) -> ParamValue:
        """Decode exactly one of the eight typed leaf variants."""
        return _decode(_PARAM_VALUE, self.leaf(raw, "ParamValue"))

    def parameter_declarations(self, raw: bytes) -> ParamDeclarations:
        """Validate dynamic parameter names as well as their declarations."""
        return _decode(_PARAM_DECLS, self.leaf(raw, "ParamDeclarations"))

    def parameter_values(self, raw: bytes) -> ParamValues:
        """Validate a complete value map without assuming a matching declaration."""
        return _decode(_PARAM_VALUES, self.leaf(raw, "ParamValues"))

    def effects_index(self, raw: bytes) -> EffectsIndex:
        """Validate index syntax and uniqueness without opening candidate files."""
        document = _document(raw)
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
