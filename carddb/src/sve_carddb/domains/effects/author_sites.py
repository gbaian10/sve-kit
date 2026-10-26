"""Preserve schema-defined syntax sites independently of runtime TypedDict erasure."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from jsonschema import Draft202012Validator
from pydantic import ConfigDict, JsonValue, TypeAdapter
from referencing import Registry, Resource

from sve_carddb.contracts.author_leaves import ParamRef
from sve_carddb.contracts.author_programs import AbilityHead
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.domains.effects.author_validation import PathParts, typed_nodes

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

BASE = "urn:sve-kit:author-nodes:1.0"
_ANCHORS = {"authorParam": "A.param", "authorGenerated": "A.ability.generated"}
_KEYWORDS = frozenset(
    [
        "$schema",
        "$id",
        "$ref",
        "$dynamicRef",
        "$anchor",
        "$dynamicAnchor",
        "$defs",
        "title",
        "description",
        "$comment",
        "default",
        "examples",
        "deprecated",
        "readOnly",
        "writeOnly",
        "discriminator",
        "type",
        "enum",
        "const",
        "required",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "minLength",
        "maxLength",
        "pattern",
        "format",
        "minItems",
        "maxItems",
        "uniqueItems",
        "minProperties",
        "maxProperties",
        "dependentRequired",
        "not",
        "propertyNames",
        "contains",
        "minContains",
        "maxContains",
        "anyOf",
        "oneOf",
        "allOf",
        "if",
        "then",
        "else",
        "properties",
        "patternProperties",
        "additionalProperties",
        "items",
        "prefixItems",
    ]
)
_HEAD = TypeAdapter(
    AbilityHead, config=ConfigDict(strict=True, regex_engine="python-re")
)


class SiteError(ValueError):
    def __init__(self, reason: str, path: PathParts = ()) -> None:
        self.reason = reason
        self.path = path
        super().__init__(reason)


@dataclass(frozen=True)
class SchemaNode:
    definition: str
    path: PathParts
    value: JsonValue


@dataclass(frozen=True)
class ParameterSite:
    path: PathParts
    name: str
    owner: str
    relative_path: PathParts
    fragment: JsonValue


@dataclass(frozen=True)
class SiteRule:
    owner: str
    path: PathParts
    type: str | None
    source_type: str = ""
    role: str = ""
    component: str = "value"


SITE_RULES = (
    SiteRule("A.expr.plus", ("plus", 0), "Int", "Nat", "arithmetic_offset"),
    SiteRule("A.expr.plus", ("plus", 1), "Int", "Nat", "arithmetic_offset"),
    SiteRule("A.expr.minus", ("minus", 1), "Int", "Nat", "arithmetic_offset"),
    SiteRule("A.expr.count-objects", ("who",), "Player", "Player", "counted_owner"),
    SiteRule("A.expr.count-objects", ("zone",), "Zone", "ZoneSet", "counted_zone"),
    SiteRule("A.sel.zone-objects", ("who",), "Player", "Player", "source_owner"),
    SiteRule("A.sel.zone-objects", ("zone",), "Zone", "ZoneSet", "source_zone"),
    SiteRule("A.sel.zone-objects", ("kind",), "CardKind", "CardKind", "filter_kind"),
    SiteRule("A.sel.zone-objects", ("class",), "Concept", "Concept", "class_filter"),
    SiteRule("A.sel.zone-objects", ("trait",), "Concept", "Concept", "trait"),
    SiteRule("A.sel.zone-objects", ("name",), "CardName", "CardName", "name_filter"),
    SiteRule("A.sel.zone-objects", ("traits_all", "*"), "Concept", "Concept", "trait"),
    SiteRule("A.sel.in-objects", ("kind",), "CardKind", "CardKind", "filter_kind"),
    SiteRule("A.sel.in-objects", ("class",), "Concept", "Concept", "class_filter"),
    SiteRule("A.sel.in-objects", ("trait",), "Concept", "Concept", "trait"),
    SiteRule("A.sel.in-objects", ("name",), "CardName", "CardName", "name_filter"),
    SiteRule("A.sel.in-objects", ("traits_all", "*"), "Concept", "Concept", "trait"),
    SiteRule(
        "A.target.exact", ("n",), "Nat", "QuantitySpec", "selection_count", "constant"
    ),
    SiteRule(
        "A.target.up-to",
        ("upto",),
        "Nat",
        "QuantitySpec",
        "selection_count",
        "constant",
    ),
    SiteRule(
        "A.act.choose.exact",
        ("n",),
        "Nat",
        "QuantitySpec",
        "selection_count",
        "constant",
    ),
    SiteRule(
        "A.act.choose.up-to",
        ("upto",),
        "Nat",
        "QuantitySpec",
        "selection_count",
        "constant",
    ),
    SiteRule(
        "A.act.search.exact",
        ("n",),
        "Nat",
        "QuantitySpec",
        "selection_count",
        "constant",
    ),
    SiteRule(
        "A.act.search.up-to",
        ("upto",),
        "Nat",
        "QuantitySpec",
        "selection_count",
        "constant",
    ),
    SiteRule("A.act.search.exact", ("to",), "Zone", "ZoneSet", "destination_zone"),
    SiteRule("A.act.search.up-to", ("to",), "Zone", "ZoneSet", "destination_zone"),
    SiteRule("A.act.move.ref", ("to",), "Zone", "ZoneSet", "destination_zone"),
    SiteRule("A.act.move.top", ("to",), "Zone", "ZoneSet", "destination_zone"),
    SiteRule("A.act.create", ("to",), "Zone", "ZoneSet", "destination_zone"),
    SiteRule("A.act.draw", ("n",), "Nat", "Nat", "count"),
    SiteRule("A.act.discard.exact", ("n",), "Nat", "Nat", "count"),
    SiteRule("A.act.create", ("n",), "Nat", "Nat", "count"),
    SiteRule("A.act.draw", ("who",), "Player", "Player", "destination_owner"),
    SiteRule("A.act.damage", ("n",), "Nat", "Nat", "damage_amount"),
    SiteRule("A.act.life", ("delta",), "Nat", "Nat", "recovery_amount"),
    SiteRule(
        "A.effect.cost-mod",
        ("delta",),
        "Int",
        "Nat",
        "cost_delta_magnitude",
        "negated_magnitude",
    ),
    SiteRule("A.effect.cost-mod", ("set",), "Nat", "Nat", "cost_value"),
    SiteRule("A.event.phase-payload", ("which",), "Phase", "Phase", "phase_trigger"),
    SiteRule("A.act.grant", ("keywords", "*"), "Concept", "Concept", "keyword"),
    # Bare keyword heads remain fragment types; companion bodies exclude declarations.
    SiteRule("A.ability-head", ("keywords", "*"), "Concept", "Concept", "keyword"),
    SiteRule("A.ability.keywords", ("keywords", "*"), "Concept", "Concept", "keyword"),
    SiteRule("A.act.create", ("token",), "CardName", "CardName", "created_name"),
    SiteRule("A.act.create", ("token", "*"), "CardName", "CardName", "created_name"),
    SiteRule("A.target.grouped.exact", ("n",), None),
    SiteRule("A.target.grouped.up-to", ("upto",), None),
    SiteRule("A.act.search.grouped", ("to",), None),
    SiteRule("A.instead.damage", ("n",), None),
    SiteRule("A.instead.move", ("to",), None),
    SiteRule("A.instead.life", ("delta",), None),
    SiteRule("A.instead.grant", ("keywords", "*"), None),
    SiteRule("A.instead.create", ("n",), None),
    SiteRule("A.instead.create", ("to",), None),
    SiteRule("A.instead.draw", ("n",), None),
    SiteRule("A.instead.search", ("n",), None),
    SiteRule("A.instead.search", ("upto",), None),
    SiteRule("A.instead.search", ("to",), None),
)


def site_rule(site: ParameterSite) -> SiteRule:
    """Require an exact owner and operand path; never search ancestors."""
    matches = [
        rule
        for rule in SITE_RULES
        if rule.owner == site.owner
        and len(rule.path) == len(site.relative_path)
        and all(
            a == b or (a == "*" and type(b) is int)
            for a, b in zip(rule.path, site.relative_path, strict=True)
        )
    ]
    if len(matches) != 1 or matches[0].type is None:
        raise SiteError("unsupported_param_site", site.path)
    return matches[0]


def _absolute(value: JsonValue) -> JsonValue:
    if isinstance(value, list):
        return [_absolute(child) for child in value]
    if isinstance(value, dict):
        return {
            key: BASE + child
            if key in {"$ref", "$dynamicRef"}
            and isinstance(child, str)
            and child.startswith("#")
            else _absolute(child)
            for key, child in value.items()
        }
    return value


class NodeSites:
    """Walk positive schema branches with fixed local references and exact paths."""

    def __init__(self, schema_directory: Path) -> None:
        self.schema = object_value(
            parse((schema_directory / "author-nodes.schema.json").read_bytes())
        )
        self.definitions = object_value(self.schema["$defs"])
        registry = Registry().with_resource(BASE, Resource.from_contents(self.schema))
        self._registry = registry
        self._validators: dict[bytes, Draft202012Validator] = {}

    def _valid(self, schema: JsonValue, value: JsonValue) -> bool:
        key = canonical(schema)
        if key not in self._validators:
            self._validators[key] = Draft202012Validator(
                schema if isinstance(schema, bool) else object_value(_absolute(schema)),
                registry=self._registry,
            )
        return bool(self._validators[key].is_valid(value))

    def _reference(self, reference: str) -> str:
        for anchor, name in _ANCHORS.items():
            if reference == BASE + ":" + anchor + "#" + anchor:
                return name
        prefix = BASE + "#/$defs/" if reference.startswith(BASE) else "#/$defs/"
        if (
            not reference.startswith(prefix)
            or reference[len(prefix) :] not in self.definitions
        ):
            raise SiteError("unsupported_schema_reference")
        return reference[len(prefix) :]

    def _children(
        self, schema: dict[str, JsonValue], value: JsonValue, path: PathParts
    ) -> Iterator[tuple[JsonValue, JsonValue, PathParts]]:
        unknown = schema.keys() - _KEYWORDS
        if unknown:
            raise SiteError("unsupported_schema_keyword", path)
        for key in ("allOf", "anyOf", "oneOf"):
            branches = schema.get(key)
            if isinstance(branches, list):
                for branch in branches:
                    if key == "allOf" or self._valid(branch, value):
                        yield branch, value, path
        if "if" in schema:
            key = "then" if self._valid(schema["if"], value) else "else"
            if key in schema:
                yield schema[key], value, path
        yield from self._object_children(schema, value, path)
        yield from self._array_children(schema, value, path)

    @staticmethod
    def _object_children(
        schema: dict[str, JsonValue], value: JsonValue, path: PathParts
    ) -> Iterator[tuple[JsonValue, JsonValue, PathParts]]:
        if isinstance(value, dict):
            properties = object_value(schema.get("properties", {}))
            patterns = object_value(schema.get("patternProperties", {}))
            for key, child in value.items():
                matched = False
                if key in properties:
                    yield properties[key], child, (*path, key)
                    matched = True
                for pattern, branch in patterns.items():
                    if re.search(pattern, key):
                        yield branch, child, (*path, key)
                        matched = True
                if not matched and "additionalProperties" in schema:
                    yield schema["additionalProperties"], child, (*path, key)

    @staticmethod
    def _array_children(
        schema: dict[str, JsonValue], value: JsonValue, path: PathParts
    ) -> Iterator[tuple[JsonValue, JsonValue, PathParts]]:
        if isinstance(value, list):
            prefix = schema.get("prefixItems", [])
            if isinstance(prefix, list):
                for index, child in enumerate(value):
                    if index < len(prefix):
                        yield prefix[index], child, (*path, index)
                    elif "items" in schema:
                        yield schema["items"], child, (*path, index)

    def _walk(  # ruff: ignore[complex-structure] -- Explicit positive-schema traversal keeps owner/path state together.
        self, value: JsonValue, definition: str
    ) -> tuple[list[SchemaNode], list[ParameterSite]]:
        nodes: list[SchemaNode] = []
        params: list[ParameterSite] = []
        # The owner changes only where a definition actually owns properties.
        pending: list[
            tuple[JsonValue, JsonValue, PathParts, str, str, PathParts, JsonValue]
        ] = [(self.definitions[definition], value, (), definition, "", (), value)]
        seen: set[tuple[bytes, PathParts, str, str, PathParts]] = set()
        while pending:
            schema, current, path, name, owner, origin, fragment = pending.pop()
            if not isinstance(schema, dict):
                continue
            if schema.keys() - _KEYWORDS:
                raise SiteError("unsupported_schema_keyword", path)
            key = (canonical(schema), path, name, owner, origin)
            if key in seen:
                continue
            seen.add(key)
            if schema is self.definitions.get(name):
                nodes.append(SchemaNode(name, path, current))
                if name == "A.param":
                    param = object_value(current).get("param")
                    if not isinstance(param, str) or not owner:
                        raise SiteError("unowned_param_site", path)
                    params.append(
                        ParameterSite(path, param, owner, path[len(origin) :], fragment)
                    )
                    continue
                if "properties" in schema:
                    owner, origin, fragment = name, path, current
            for ref_key in ("$ref", "$dynamicRef"):
                reference = schema.get(ref_key)
                if isinstance(reference, str):
                    target = self._reference(reference)
                    pending.append(
                        (
                            self.definitions[target],
                            current,
                            path,
                            target,
                            owner,
                            origin,
                            fragment,
                        )
                    )
            for child, instance, location in self._children(schema, current, path):
                pending.append(
                    (child, instance, location, name, owner, origin, fragment)
                )
        return nodes, params

    def walk(
        self, value: JsonValue, definition: str = "A.ability-head"
    ) -> tuple[SchemaNode, ...]:
        """Return all reached definition nodes; this is not source evidence."""
        if definition not in self.definitions or not self._valid(
            {"$ref": BASE + "#/$defs/" + definition}, value
        ):
            raise SiteError("invalid_syntax_tree")
        nodes, _ = self._walk(value, definition)
        return tuple(nodes)

    @staticmethod
    def decode_head(body: JsonValue) -> AbilityHead:
        """Decode a head for local binding analysis without serializing defaults."""
        return _HEAD.validate_json(canonical(body), strict=True)

    def parameters(self, body: JsonValue) -> tuple[ParameterSite, ...]:
        """Cross-check schema sites against the independent decoded ParamRef graph."""
        decoded = _HEAD.validate_json(canonical(body), strict=True)
        if not self._valid({"$ref": BASE + "#/$defs/A.ability-head"}, body):
            raise SiteError("invalid_syntax_tree")
        _, params = self._walk(body, "A.ability-head")
        found: dict[PathParts, ParameterSite] = {}
        for site in params:
            previous = found.get(site.path)
            if previous is not None and (previous.owner, previous.relative_path) != (
                site.owner,
                site.relative_path,
            ):
                raise SiteError("ambiguous_param_site", site.path)
            found[site.path] = site
        paths = {
            path for path, node in typed_nodes(decoded) if isinstance(node, ParamRef)
        }
        if paths != found.keys():
            raise SiteError("param_site_drift")
        return tuple(found[path] for path in sorted(found, key=str))
