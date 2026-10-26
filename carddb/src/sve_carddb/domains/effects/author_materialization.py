"""Materialize handwritten positions into the exact existing author semantic model."""

from copy import deepcopy
from dataclasses import dataclass, replace
from functools import cached_property
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.domains.effects.author_handwritten import (
    Handwritten,
    Locator,
    decode,
    materialize,
    positions,
    resolve,
)
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    parse_document,
)
from sve_carddb.domains.effects.author_sites import NodeSites, SiteError
from sve_carddb.domains.effects.author_source_hash import source_version
from sve_carddb.domains.effects.author_sources import SourceContext, SourceError

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_current import (
        CurrentAuthorSources,
        CurrentCard,
    )
    from sve_carddb.domains.effects.author_validation import PathParts

_EMPTY_PORTS: dict[str, JsonValue] = {"imports": {}, "exports": {}}
_LOCATORS = frozenset({"line", "face", "section", "scope"})
_USE = frozenset({"use", "params", "edition", "registration"})
_CARD_KEYS = frozenset({"meta", "abilities", "note", "deck_rule"})


@dataclass(frozen=True)
class MaterializationContext:
    current: CurrentAuthorSources
    schema_directory: Path
    card: CurrentCard

    @cached_property
    def sources(self) -> tuple[SourceContext, ...]:
        """Verify once within this card operation, after resolving any ambiguity."""
        return self.current.checked_sources(self.card)

    @cached_property
    def contexts(self) -> dict[tuple[str, str], SourceContext]:
        """Reuse checked contexts across sibling abilities in this operation."""
        return self.current.contexts(self.sources)


def _body(
    value: dict[str, JsonValue],
    handwritten: Handwritten,
    source: SourceContext,
    scope: str,
    sites: NodeSites,
) -> tuple[dict[str, JsonValue], int]:
    name = string(value["use"])
    editions = [
        edition for identifier, edition in handwritten.bodies if identifier == name
    ]
    edition = value.get("edition")
    if edition is None:
        if len(editions) != 1:
            raise SourceError("ambiguous_body_version" if editions else "body_missing")
        edition = editions[0]
    if type(edition) is not int or edition < 1:
        raise SourceError("invalid_body_edition")
    definition = handwritten.bodies.get((name, edition))
    if definition is None:
        raise SourceError("body_missing")
    if definition.ability_scope != scope:
        raise SourceError("body_scope_mismatch")
    try:
        return materialize(definition, source, sites), edition
    except SiteError as error:
        raise AuthorInputError(Diagnostic("source", error.reason, error.path)) from None
    except ValidationError as error:
        path = tuple(error.errors(include_input=False, include_url=False)[0]["loc"])
        raise AuthorInputError(
            Diagnostic("schema", "invalid_syntax_tree", path)
        ) from None


def _ability(
    value: dict[str, JsonValue],
    card: CurrentCard,
    handwritten: Handwritten,
    context: MaterializationContext,
    sites: NodeSites,
) -> tuple[str, dict[str, JsonValue], dict[str, JsonValue] | None, int]:
    locator = decode(
        parse_document(
            canonical({key: item for key, item in value.items() if key in _LOCATORS})
        ),
        Locator,
    )
    if (
        isinstance(locator.face, str)
        and sum(side == locator.face for _, side in card.face_positions.values()) > 1
    ):
        raise SourceError("ambiguous_face_alias")
    located = resolve(
        positions(card, context.current.classifier),
        locator,
        keywords="keywords" in value,
    )
    occurrence = located.source.binding.occurrence_key()
    source: dict[str, JsonValue] = {
        "binding_id": located.source.binding.id,
        "occurrence": occurrence.model_dump(mode="json"),
    }
    result: dict[str, JsonValue] = {"line": locator.line, "sources": [source]}
    if occurrence.ordinal is not None:
        result["section"] = occurrence.ordinal
    body = None
    edition = 1
    if "use" in value:
        if value.keys() - (_LOCATORS | _USE):
            raise SourceError("ability_head_conflict")
        assert located.ability_scope is not None
        # Obtain the checked classifier, not the inspection-only derived context.
        contexts = context.contexts
        source_context = contexts[located.source.binding.id, located.ability_scope]
        body, edition = _body(
            value, handwritten, source_context, located.ability_scope, sites
        )
        source["ability_scope"] = located.ability_scope
        params = object_value(value.get("params", {}))
        declarations = object_value(body["params"])
        if params.keys() != declarations.keys():
            raise SourceError("parameter_name_mismatch")
        result["generated_by"] = {
            "macro": {
                "frame_id": body["frame_id"],
                "ability_scope": body["ability_scope"],
                "applicability_scope": value.get("registration", "jp"),
            },
            "version": body["body_version"],
            "params": {
                name: {"type": object_value(declarations[name])["type"], "value": item}
                for name, item in params.items()
            },
        }
    else:
        if value.keys() & _USE:
            raise SourceError("ability_head_conflict")
        if located.ability_scope is not None:
            source["ability_scope"] = located.ability_scope
        result.update(
            {key: item for key, item in value.items() if key not in _LOCATORS}
        )
        result.setdefault("ports", deepcopy(_EMPTY_PORTS))
    return located.face_id, result, body, edition


def materialize_card(  # ruff: ignore[too-many-locals] -- keep the complete ordered boundary in one operation
    context: MaterializationContext,
    number: str,
    handwritten: Handwritten,
    *,
    container: str = "faces",
) -> tuple[
    str,
    dict[str, JsonValue],
    list[tuple[dict[str, JsonValue], int]],
    dict[PathParts, int],
]:
    """Resolve all positions and preserve the existing exact semantic container."""
    value = handwritten.cards[number]
    if value.keys() - _CARD_KEYS or not {"meta", "abilities"} <= value.keys():
        raise SourceError("invalid_card_keys")
    meta = object_value(value["meta"])
    if meta.keys() - {
        "written_by",
        "reviewed_by",
        "status",
        "verified_by_exam",
        "rulings",
    } or not meta.get("written_by"):
        raise SourceError("invalid_card_metadata")
    card_id = context.current.printing_card(number)
    card = context.card
    generated_meta: dict[str, JsonValue] = {
        "dsl": "1.0",
        "source": source_version(card.payload),
        "written_by": meta["written_by"],
        "reviewed_by": meta.get("reviewed_by", []),
        "status": meta.get("status", "draft"),
        "verified_by_exam": meta.get("verified_by_exam", []),
        "rulings": meta.get("rulings", []),
    }
    result: dict[str, JsonValue] = {"meta": generated_meta}
    for key in ("note", "deck_rule"):
        if key in value:
            result[key] = value[key]
    use_faces = container == "faces"
    result["faces" if use_faces else "abilities"] = {} if use_faces else []
    bodies: list[tuple[dict[str, JsonValue], int]] = []
    locations: dict[PathParts, int] = {}
    sites = NodeSites(context.schema_directory)
    for index, ability in enumerate(array(value["abilities"])):
        try:
            face, expanded, body, edition = _ability(
                object_value(ability), card, handwritten, context, sites
            )
            if use_faces:
                face_container = object_value(result["faces"])
                face_card = object_value(
                    face_container.setdefault(face, {"abilities": []})
                )
                locations[
                    "cards",
                    number,
                    "faces",
                    face,
                    "abilities",
                    len(array(face_card["abilities"])),
                ] = index
                array(face_card["abilities"]).append(expanded)
            else:
                locations["cards", number, "abilities", index] = index
                array(result["abilities"]).append(expanded)
            if body is not None:
                bodies.append((body, edition))
        except (AuthorInputError, SourceError) as error:
            diagnostic = (
                error.diagnostic
                if isinstance(error, AuthorInputError)
                else Diagnostic("source", error.reason)
            )
            path = ("cards", number, "abilities", index, *diagnostic.path)
            cursor = handwritten.documents[number].diagnostic(
                diagnostic.stage,
                diagnostic.reason,
                path,
                diagnostic.expected,
                diagnostic.actual,
            )
            line = object_value(ability).get("line")
            raise AuthorInputError(
                replace(
                    cursor,
                    card_no=number,
                    card_id=card_id,
                    card_line=line if type(line) is int else None,
                )
            ) from None
    return card_id, result, bodies, locations
