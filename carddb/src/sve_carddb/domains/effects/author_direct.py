"""Expand named bodies against card coordinates, without translation-frame proofs."""

from copy import deepcopy
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.domains.effects.author_expansion import (
    ExpansionCandidate,
    ability_nodes,
)
from sve_carddb.domains.effects.author_handwritten import Locator, RuleSection, decode
from sve_carddb.domains.effects.author_inputs import parse_document
from sve_carddb.domains.effects.author_sources import SourceError
from sve_carddb.domains.translations.four_layer_sources import card_field

if TYPE_CHECKING:
    from sve_carddb.build import Value
    from sve_carddb.domains.effects.author_current import CurrentAuthorSources
    from sve_carddb.domains.effects.author_expansion import AuthorExpander
    from sve_carddb.domains.effects.author_handwritten import Handwritten

_LOCATION = frozenset({"line", "face", "section", "scope", "covers", "admission"})
_USE = frozenset({"use", "edition", "params", "registration"})
_CARD_KEYS = frozenset(
    {
        "admission",
        "meta",
        "abilities",
        "ignored",
        "note",
        "deck_rule",
        "container",
        "token",
        "token_sources",
        "rule_sections",
    }
)
_META_KEYS = frozenset(
    {"written_by", "reviewed_by", "status", "verified_by_exam", "rulings"}
)
_EMPTY_PORTS: dict[str, JsonValue] = {"imports": {}, "exports": {}}


def _scalar(value: Value) -> str | int | None:
    if value is None or type(value) in {str, int}:
        assert value is None or isinstance(value, str | int)
        return value
    raise SourceError("source_database_shape")


def direct(handwritten: Handwritten, number: str) -> bool:
    """An explicit card, use or body admission selects the direct author path."""
    card = handwritten.cards[number]
    return "admission" in card or any(
        "admission" in (value := object_value(a))
        or any(
            body.admission is not None
            for (name, _), body in handwritten.bodies.items()
            if name == value.get("use")
        )
        for a in array(card["abilities"])
    )


def inventory(  # ruff: ignore[complex-structure] -- keep permanent identity and its complete line inventory at one boundary
    current: CurrentAuthorSources, number: str, *, card_id: str | None = None
) -> dict[str, JsonValue]:
    """Read permanent identities and actual nonempty lines independently of templates."""
    selected_id = current.printing_card(number)
    if card_id is not None and card_id != selected_id:
        raise SourceError("record_key_identity_mismatch")
    card_id = selected_id
    db = current.db
    cards = db.select("card", ("identity_state",), where={"id": card_id})
    if len(cards) != 1 or cards[0].values["identity_state"] != "confirmed":
        raise SourceError("source_identity_unconfirmed")
    faces: list[JsonValue] = []
    for row in db.select("face", db.columns("face"), where={"card_id": card_id}):
        face = row.values
        selected = db.select(
            "face_current",
            ("revision_id",),
            where={"face_id": face["id"], "region": "jp"},
        )
        if len(selected) != 1:
            raise SourceError("source_current_missing")
        revision_id = selected[0].values["revision_id"]
        if not isinstance(revision_id, str):
            raise SourceError("source_current_missing")
        revisions = db.select(
            "face_revision", ("cost", "face_id", "region"), where={"id": revision_id}
        )
        if (
            len(revisions) != 1
            or revisions[0].values["face_id"] != face["id"]
            or revisions[0].values["region"] != "jp"
        ):
            raise SourceError("source_current_mismatch")
        sections = db.select(
            "face_text_section", ("ordinal", "kind"), where={"revision_id": revision_id}
        )
        ordinals = [s.values["ordinal"] for s in sections]
        if ordinals != list(range(len(sections))):
            raise SourceError("source_sections_incomplete")
        fields: list[JsonValue] = []
        for ordinal in (None, *range(len(sections))):
            field = "effect" if ordinal is None else "section"
            selected_field = card_field(
                db,
                OwnerField(
                    owner=FaceRevisionOwner(
                        kind="face_revision", revision_id=revision_id
                    ),
                    field=field,
                    ordinal=ordinal,
                ),
            )
            entry: dict[str, JsonValue] = {
                "field": field,
                "ordinal": ordinal,
                "lines": [
                    i + 1
                    for i, line in enumerate(selected_field.text.splitlines())
                    if line.strip()
                ],
            }
            if ordinal is not None:
                entry["section_kind"] = _scalar(sections[ordinal].values["kind"])
            fields.append(entry)
        faces.append(
            {
                "face_id": _scalar(face["id"]),
                "revision_id": revision_id,
                "ordinal": _scalar(face["ordinal"]),
                "side": _scalar(face["side"]),
                "cost": _scalar(revisions[0].values["cost"]),
                "fields": fields,
            }
        )
    if not faces:
        raise SourceError("source_current_missing")
    return {"card_id": card_id, "faces": faces}


def _position(
    payload: dict[str, JsonValue], value: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    locator = decode(
        parse_document(
            canonical(
                {
                    k: v
                    for k, v in value.items()
                    if k in {"line", "face", "section", "scope"}
                }
            )
        ),
        Locator,
    )
    face, field = _field(payload, locator)
    if locator.line not in array(field["lines"]):
        raise SourceError("source_location_mismatch")
    return {
        "owner": {"kind": "face_revision", "revision_id": face["revision_id"]},
        "face_id": face["face_id"],
        "field": field["field"],
        "ordinal": field["ordinal"],
        "line": locator.line,
        "scope": f"ability_{(locator.scope or 1) - 1}",
        "covers": value.get("covers", [locator.line]),
    }


def _field(
    payload: dict[str, JsonValue], locator: Locator | RuleSection
) -> tuple[dict[str, JsonValue], dict[str, JsonValue]]:
    faces = [object_value(f) for f in array(payload["faces"])]
    if locator.face is not None:
        faces = [
            f
            for f in faces
            if (type(locator.face) is int and f["ordinal"] == locator.face - 1)
            or f["side"] == locator.face
        ]
    if len(faces) != 1:
        raise SourceError("ambiguous_face" if faces else "missing_face")
    face = faces[0]
    ordinal = locator.section - 1 if type(locator.section) is int else None
    fields = [
        object_value(f)
        for f in array(face["fields"])
        if object_value(f)["ordinal"] == ordinal
    ]
    if len(fields) != 1:
        raise SourceError("source_location_mismatch")
    return face, fields[0]


def _rule_sections(
    payload: dict[str, JsonValue],
    card: dict[str, JsonValue],
    faces: dict[str, JsonValue],
) -> set[tuple[str, int]]:
    declarations = array(card.get("rule_sections", []))
    if not declarations:
        return set()
    covered = {
        (string(source["face_id"]), source["ordinal"], line)
        for _, ability in ability_nodes({"cards": {"card": {"faces": faces}}})
        for source in [object_value(array(ability["sources"])[0])]
        for line in array(source["covers"])
    }
    declared: set[tuple[str, int]] = set()
    for raw in declarations:
        locator = decode(parse_document(canonical(raw)), RuleSection)
        face, field = _field(payload, locator)
        key = (string(face["face_id"]), locator.section - 1)
        if key in declared:
            raise SourceError("duplicate_rule_section")
        if field.get("section_kind") not in {"unknown", "rule"}:
            raise SourceError("rule_section_kind_mismatch")
        lines = array(field["lines"])
        if not lines or any((*key, line) not in covered for line in lines):
            raise SourceError("source_line_closure_mismatch")
        declared.add(key)
    return declared


def _check_unknown_sections(
    current: CurrentAuthorSources,
    payload: dict[str, JsonValue],
    reminders: list[dict[str, JsonValue]],
    tokens: list[dict[str, JsonValue]],
    rule_sections: set[tuple[str, int]],
) -> None:
    ignored = {
        (source["face_id"], source["ordinal"], source["line"]) for source in reminders
    }
    linked = {
        (source["face_id"], source["ordinal"], line)
        for token in tokens
        for source in [object_value(token["source"])]
        for line in array(source["covers"])
    }
    for raw in array(payload["faces"]):
        face = object_value(raw)
        sections = {
            row.values["ordinal"]: row.values["kind"]
            for row in current.db.select(
                "face_text_section",
                ("ordinal", "kind"),
                where={"revision_id": string(face["revision_id"])},
            )
        }
        for raw_field in array(face["fields"]):
            field = object_value(raw_field)
            kind = next(
                (
                    kind
                    for ordinal, kind in sections.items()
                    if ordinal == field["ordinal"]
                ),
                None,
            )
            if (
                kind == "unknown"
                and (face["face_id"], field["ordinal"]) in rule_sections
            ):
                continue
            if kind in {"unknown", "token_definition"} and any(
                (key := (face["face_id"], field["ordinal"], line)) not in linked
                and (kind != "unknown" or key not in ignored)
                for line in array(field["lines"])
            ):
                raise SourceError("token_source_contract_unavailable")


def _token_sources(
    payload: dict[str, JsonValue], card: dict[str, JsonValue]
) -> list[dict[str, JsonValue]]:
    tokens: list[dict[str, JsonValue]] = []
    for raw in array(card.get("token_sources", [])):
        value = object_value(raw)
        if value.keys() - {"card", "line", "section", "face", "covers"} or not (
            isinstance(value.get("card"), str) and value["card"]
        ):
            raise SourceError("invalid_token_source")
        source = _position(payload, value)
        matching = [
            object_value(field)
            for face in array(payload["faces"])
            if object_value(face)["face_id"] == source["face_id"]
            for field in array(object_value(face)["fields"])
            if object_value(field)["ordinal"] == source["ordinal"]
        ]
        if source["field"] != "section" or matching[0].get("section_kind") not in {
            "unknown",
            "token_definition",
        }:
            raise SourceError("invalid_token_source")
        tokens.append(
            {"source": {**source, "admission": "light"}, "card_no": value["card"]}
        )
    return tokens


def _substitute(
    node: JsonValue, values: dict[str, JsonValue], used: set[str]
) -> JsonValue:
    if isinstance(node, dict):
        if set(node) == {"param"}:
            name = string(node["param"])
            if name not in values:
                raise SourceError("parameter_name_mismatch")
            used.add(name)
            return values[name]
        return {k: _substitute(v, values, used) for k, v in node.items()}
    if isinstance(node, list):
        return [_substitute(v, values, used) for v in node]
    return node


def _body(
    value: dict[str, JsonValue], handwritten: Handwritten
) -> tuple[dict[str, JsonValue], str | None]:
    if "use" not in value:
        return {k: v for k, v in value.items() if k not in _LOCATION}, None
    if value.keys() - (_LOCATION | _USE):
        raise SourceError("ability_head_conflict")
    definitions = [
        b
        for (name, edition), b in handwritten.bodies.items()
        if name == value["use"]
        and ("edition" not in value or edition == value["edition"])
    ]
    if len(definitions) != 1:
        raise SourceError("ambiguous_body_version" if definitions else "body_missing")
    definition = definitions[0]
    if any(
        object_value(r).get("verdict") in {"disputed", "revise"}
        for registration in definition.registrations
        for r in array(object_value(registration).get("reviews", []))
    ):
        raise SourceError("source_unresolved_issue")
    values = object_value(value.get("params", {}))
    if values.keys() != definition.params.keys():
        raise SourceError("parameter_name_mismatch")
    for name, parameter in definition.params.items():
        kind = parameter if isinstance(parameter, str) else parameter.type
        item = values[name]
        numeric = kind in {"Nat", "Int"}
        valid = type(item) is int if numeric else isinstance(item, str) and bool(item)
        if kind == "Nat" and type(item) is int:
            valid = item >= 0
        if not valid:
            raise SourceError("param_type_mismatch")
    used: set[str] = set()
    body = object_value(_substitute(definition.body, values, used))
    if used != values.keys():
        raise SourceError("unused_param_declaration")
    return {**body, "ports": definition.ports}, definition.admission


def materialize(
    current: CurrentAuthorSources, number: str, handwritten: Handwritten
) -> dict[str, JsonValue]:
    """Keep source coordinates explicit while allowing equivalent wording to share a body."""
    card = handwritten.cards[number]
    if card.keys() - _CARD_KEYS or not {"meta", "abilities"} <= card.keys():
        raise SourceError("invalid_card_keys")
    meta = object_value(card["meta"])
    if meta.keys() - _META_KEYS or not meta.get("written_by"):
        raise SourceError("invalid_card_metadata")
    if meta.get("status") == "disputed" or any(
        object_value(r).get("verdict") in {"disputed", "revise"}
        for r in array(meta.get("reviewed_by", []))
    ):
        raise SourceError("source_unresolved_issue")
    payload = inventory(current, number)
    faces: dict[str, JsonValue] = {}
    for raw in array(card["abilities"]):
        value = object_value(raw)
        source = _position(payload, value)
        body, admission = _body(value, handwritten)
        source["admission"] = value.get(
            "admission", admission or card.get("admission", "shared")
        )
        body.setdefault("ports", deepcopy(_EMPTY_PORTS))
        ability: dict[str, JsonValue] = {
            **body,
            "line": source["line"],
            "sources": [source],
        }
        if source["ordinal"] is not None:
            ability["section"] = source["ordinal"]
        container = object_value(
            faces.setdefault(string(source["face_id"]), {"abilities": []})
        )
        array(container["abilities"]).append(ability)
    ignored = [
        _position(payload, object_value(v)) for v in array(card.get("ignored", []))
    ]
    tokens = _token_sources(payload, card)
    _check_unknown_sections(
        current,
        payload,
        [
            source
            for raw, source in zip(array(card.get("ignored", [])), ignored, strict=True)
            if object_value(raw)["reason"] == "reminder"
        ],
        tokens,
        _rule_sections(payload, card, faces),
    )
    result: dict[str, JsonValue] = {
        "meta": {
            "dsl": "1.0",
            "source": "direct",
            "admission": card.get("admission", "light"),
            "written_by": meta["written_by"],
            "status": meta.get("status", "draft"),
            "reviewed_by": meta.get("reviewed_by", []),
            "verified_by_exam": [],
            "rulings": meta.get("rulings", []),
        },
        "faces": faces,
    }
    # Inert lines are handoff inventory, never executable DSL nodes.
    result["note"] = card.get("note", "Direct author admission")
    if "deck_rule" in card:
        result["deck_rule"] = card["deck_rule"]
    if "token" in card:
        result["token"] = card["token"]
    if tokens:
        result["token_sources"] = list[JsonValue](tokens)
    result["ignored"] = [{**source, "admission": "light"} for source in ignored]
    check_coverage(payload, result, ignored)
    return result


def check_coverage(
    payload: dict[str, JsonValue], card: JsonValue, ignored: list[dict[str, JsonValue]]
) -> None:
    """Cover each real line and reject duplicate author coordinates."""
    expected = {
        (string(f["face_id"]), field["ordinal"], line)
        for raw in array(payload["faces"])
        for f in [object_value(raw)]
        for raw_field in array(f["fields"])
        for field in [object_value(raw_field)]
        for line in array(field["lines"])
    }
    covered: set[tuple[str, JsonValue, JsonValue]] = set()
    identities: set[str] = set()
    for _, ability in ability_nodes({"cards": {"card": card}}):
        source = object_value(array(ability["sources"])[0])
        key = canonical(
            {k: source[k] for k in ("face_id", "ordinal", "line", "scope")}
        ).decode()
        if key in identities or source["line"] not in array(source["covers"]):
            raise SourceError("duplicate_source_ability")
        identities.add(key)
        covered.update(
            (string(source["face_id"]), source["ordinal"], line)
            for line in array(source["covers"])
        )
    for source in ignored:
        ignored_key = (string(source["face_id"]), source["ordinal"], source["line"])
        if ignored_key in covered:
            raise SourceError("duplicate_source_ability")
        covered.add(ignored_key)
    for token in array(object_value(card).get("token_sources", [])):
        source = object_value(object_value(token)["source"])
        keys = {
            (string(source["face_id"]), source["ordinal"], line)
            for line in array(source["covers"])
        }
        if covered & keys:
            raise SourceError("duplicate_source_ability")
        covered.update(keys)
    if covered != expected:
        raise SourceError("source_line_closure_mismatch")


def candidate(tree: JsonValue, expander: AuthorExpander) -> ExpansionCandidate:
    """Expanded-schema and ordinary lowering retain all structural checks."""
    expander.schemas.expanded(canonical(tree))
    return ExpansionCandidate(tree, ())
