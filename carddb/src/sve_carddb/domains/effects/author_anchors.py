"""Reconstruct exact author semantics and compare committed per-owner anchors."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value, string
from sve_carddb.domains.effects.author_anchor_basis import (
    RECIPE,
    body_anchor,
    build_card,
    compare_card,
)
from sve_carddb.domains.effects.author_expansion import body_version
from sve_carddb.domains.effects.author_handwritten import (
    BodyFile,
    Handwritten,
    MainFile,
    decode,
)
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    parse_document,
)
from sve_carddb.domains.effects.author_set import AuthorInputSet, InputFile
from sve_carddb.domains.effects.author_sources import SourceError

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_current import CurrentAuthorSources
    from sve_carddb.domains.effects.author_expansion import AuthorExpander

STORAGE_FORMAT = 2


def input_file(
    path: str, tree: JsonValue, *, input_hash: str | None = None
) -> InputFile:
    """Serialize an internal semantic view without writing a derived card table."""
    raw = canonical(tree)
    return InputFile(path, raw, input_hash or digest(raw))


def is_handwritten(inputs: AuthorInputSet) -> bool:
    """Identify the single storage format of a structurally checked inventory."""
    return bool(inputs.author_files or inputs.body_files) and all(
        object_value(parse_document(file.raw).value).get("format") == STORAGE_FORMAT
        for file in (*inputs.author_files, *inputs.body_files)
    )


def handwriting(inputs: AuthorInputSet) -> Handwritten:
    """Load the cross-shard card and body name tables from immutable inputs."""
    cards = {}
    bodies = {}
    documents = {}
    for file in inputs.author_files:
        document = parse_document(file.raw)
        parsed = decode(document, MainFile)
        cards.update(parsed.cards)
        documents.update(dict.fromkeys(parsed.cards, document))
    for file in inputs.body_files:
        for body in decode(parse_document(file.raw), BodyFile).records:
            bodies[body.id, body.edition] = body
    return Handwritten(cards, bodies, documents)


def _unused_body(
    definition: JsonValue, pin: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    body = object_value(definition)
    declarations: dict[str, JsonValue] = {}
    for name, parameter in object_value(body.get("params", {})).items():
        saved = object_value(pin["params"]).get(name)
        old = object_value(saved) if saved is not None else {}
        declaration = (
            {"type": parameter}
            if isinstance(parameter, str)
            else object_value(parameter)
        )
        if "source" not in declaration and "source" not in old:
            raise AuthorInputError(Diagnostic("body", "missing_parameter_source"))
        declarations[name] = {
            **declaration,
            "source": declaration.get("source", old.get("source")),
        }
    record: dict[str, JsonValue] = {
        key: body[key] for key in ("body", "written_by", "ports", "registrations")
    }
    record.update(
        {
            "dsl": "1.0",
            "frame_id": pin["frame_id"],
            "ability_scope": body["ability_scope"],
            "params": declarations,
        }
    )
    if body.get("note") is not None:
        record["note"] = body["note"]
    record["body_version"] = body_version(record)
    return record


@dataclass(frozen=True)
class Materialized:
    inputs: AuthorInputSet
    anchors: dict[str, dict[str, JsonValue]]
    failures: dict[str, Diagnostic]


def materialize_inputs(  # ruff: ignore[too-many-locals, too-many-statements, too-many-branches, complex-structure] -- reconstruct the whole inventory before granting any source proof
    inputs: AuthorInputSet,
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    *,
    refresh: frozenset[str] = frozenset(),
    refresh_unused: bool = False,
) -> Materialized:
    """Recompute independently; refresh is reserved for the explicit writer.

    Unused bodies have no selected card to justify a refresh, so only a
    write over the whole handwritten set may rebind their stale pins.
    """
    handwritten = handwriting(inputs)
    old_cards: dict[str, dict[str, JsonValue]] = {}
    old_bodies: dict[str, dict[str, JsonValue]] = {}
    body_owners = {
        f"{body.id}/{body.edition}": file.path.split("/")[-2]
        for file in inputs.body_files
        for body in decode(parse_document(file.raw), BodyFile).records
    }
    allowed_owners = {n.split("-", 1)[0] for n in handwritten.cards} | set(
        body_owners.values()
    )
    for file in inputs.anchor_files:
        tree = object_value(
            expander.schemas.storage_tree(file.raw, "effect_anchor_set")
        )
        if tree["owner"] not in allowed_owners:
            raise AuthorInputError(Diagnostic("anchor", "extra_anchor_owner"))
        for number, value in object_value(tree["cards"]).items():
            if number.split("-", 1)[0] != tree["owner"] or number in old_cards:
                raise AuthorInputError(
                    Diagnostic("anchor", "anchor_card_owner", card_no=number)
                )
            old_cards[number] = object_value(value)
        for key, value in object_value(tree["bodies"]).items():
            if key in body_owners and body_owners[key] != tree["owner"]:
                raise AuthorInputError(Diagnostic("anchor", "body_anchor_owner"))
            if key in old_bodies:
                raise AuthorInputError(Diagnostic("anchor", "duplicate_body_anchor"))
            old_bodies[key] = object_value(value)
    if old_cards.keys() - handwritten.cards.keys():
        number = min(old_cards.keys() - handwritten.cards.keys())
        raise AuthorInputError(
            Diagnostic("anchor", "extra_card_anchor", card_no=number)
        )
    expected_body_keys = {f"{name}/{edition}" for name, edition in handwritten.bodies}
    if old_bodies.keys() - expected_body_keys:
        raise AuthorInputError(Diagnostic("anchor", "extra_body_anchor"))
    anchors: dict[str, dict[str, JsonValue]] = {}
    failures = {}
    legacy_cards: dict[str, JsonValue] = {}
    legacy_bodies: dict[str, dict[str, JsonValue]] = {}
    for record in inputs.index.records:
        number = record.record_key
        if record.kind != "author_effect_set":
            continue
        pin = old_cards.get(number)
        if pin is None and number not in refresh:
            failures[number] = Diagnostic(
                "anchor", "missing_card_anchor", card_no=number
            )
            continue
        container = (
            string(object_value(pin["legacy_shape"])["container"]) if pin else "faces"
        )
        try:
            card, card_anchor = build_card(
                current, number, handwritten, container, expander, legacy_bodies
            )
            owner = number.split("-", 1)[0]
            anchor = anchors.setdefault(
                owner,
                {
                    "format": 1,
                    "kind": "effect_anchor_set",
                    "owner": owner,
                    "recipe": RECIPE,
                    "cards": {},
                    "bodies": {},
                },
            )
            object_value(anchor["cards"])[number] = card_anchor
            legacy_cards[number] = card
            if number not in refresh and pin is not None:
                failure = compare_card(pin, card_anchor, number)
                if failure is not None:
                    failures[number] = failure
        except (AuthorInputError, SourceError) as error:
            failure = (
                error.diagnostic
                if isinstance(error, AuthorInputError)
                else Diagnostic("source", error.reason)
            )
            failures[number] = replace(failure, card_no=number, card_id=record.card_id)
    body_owner = {}
    for file in inputs.body_files:
        for definition in decode(parse_document(file.raw), BodyFile).records:
            key = f"{definition.id}/{definition.edition}"
            body_owner[key] = file.path.split("/")[-2]
            if key not in legacy_bodies:
                if (
                    _body_users(handwritten, definition.id, definition.edition)
                    & failures.keys()
                ):
                    continue
                pin = old_bodies.get(key)
                if pin is None:
                    raise AuthorInputError(
                        Diagnostic("body", "unbound_body_definition")
                    )
                legacy_bodies[key] = _unused_body(
                    definition.model_dump(mode="json"), pin
                )
            try:
                pin = body_anchor(legacy_bodies[key], current, expander)
            except SourceError as error:
                affected = _body_users(handwritten, definition.id, definition.edition)
                if not affected:
                    raise AuthorInputError(Diagnostic("body", error.reason)) from None
                for number in affected:
                    failures.setdefault(
                        number, Diagnostic("body", error.reason, card_no=number)
                    )
                del legacy_bodies[key]
                continue
            owner = body_owner[key]
            anchor = anchors.setdefault(
                owner,
                {
                    "format": 1,
                    "kind": "effect_anchor_set",
                    "owner": owner,
                    "recipe": RECIPE,
                    "cards": {},
                    "bodies": {},
                },
            )
            object_value(anchor["bodies"])[key] = pin
            if old_bodies.get(key) != pin:
                affected = _body_users(handwritten, definition.id, definition.edition)
                if affected - refresh:
                    for number in affected - refresh:
                        failures.setdefault(
                            number,
                            Diagnostic(
                                "body",
                                "stale_body_anchor"
                                if key in old_bodies
                                else "missing_body_anchor",
                                card_no=number,
                            ),
                        )
                elif not affected and key in old_bodies and not refresh_unused:
                    raise AuthorInputError(Diagnostic("body", "stale_body_anchor"))
    for anchor in anchors.values():
        expander.schemas.storage_tree(canonical(anchor), "effect_anchor_set")
    authors = []
    for file in inputs.author_files:
        tree = object_value(parse_document(file.raw).value)
        cards = object_value(tree["cards"])
        selected: dict[str, JsonValue] = {
            number: legacy_cards[number] for number in cards if number in legacy_cards
        }
        authors.append(
            input_file(
                file.path,
                {
                    "format": 1,
                    "kind": "author_effect_set",
                    "scope": "shared",
                    "cards": selected,
                },
                input_hash=file.input_hash,
            )
        )
    bodies = []
    for file in inputs.body_files:
        records = [
            legacy_bodies[f"{body.id}/{body.edition}"]
            for body in decode(parse_document(file.raw), BodyFile).records
            if f"{body.id}/{body.edition}" in legacy_bodies
        ]
        bodies.append(
            input_file(
                file.path,
                {
                    "format": 1,
                    "kind": "effect_body_shard",
                    "scope": "shared",
                    "records": list[JsonValue](records),
                },
            )
        )
    for number, failure in tuple(failures.items()):
        values = array(handwritten.cards[number]["abilities"])
        first_line = object_value(values[0]).get("line") if values else None
        card_line = failure.card_line or (
            first_line if type(first_line) is int else None
        )
        cursor_path = (
            failure.path if failure.path[:2] == ("cards", number) else ("cards", number)
        )
        cursor = handwritten.documents[number].diagnostic(
            failure.stage, failure.reason, cursor_path
        )
        failures[number] = replace(
            failure,
            line=failure.line if failure.line is not None else cursor.line,
            column=failure.column if failure.column is not None else cursor.column,
            card_line=card_line,
        )
    return Materialized(
        replace(
            inputs,
            author_files=tuple(authors),
            body_files=tuple(bodies),
            anchor_files=(),
        ),
        anchors,
        failures,
    )


def review_states(inputs: AuthorInputSet) -> list[JsonValue]:
    """Report literal review freshness without inventing review qualification."""
    pins = _review_pins(inputs)
    reports: list[JsonValue] = []
    for file in inputs.author_files:
        tree = object_value(parse_document(file.raw).value)
        for number, value in object_value(tree["cards"]).items():
            for index, review in enumerate(
                array(object_value(object_value(value)["meta"]).get("reviewed_by", []))
            ):
                node = object_value(review)
                matches = node.get("anchor") is not None and node.get(
                    "anchor"
                ) == pins.get(("cards", number))
                reports.append(
                    {
                        "file": file.path,
                        "card_no": number,
                        "review": index,
                        "state": _review_state(node, matches),
                        "anchor_matches": matches,
                    }
                )
    for file in inputs.body_files:
        for definition in decode(parse_document(file.raw), BodyFile).records:
            key = f"{definition.id}/{definition.edition}"
            for registration in definition.registrations:
                node = object_value(registration)
                for index, review in enumerate(array(node.get("reviews", []))):
                    evidence = object_value(review)
                    matches = evidence.get("anchor") is not None and evidence.get(
                        "anchor"
                    ) == pins.get(("bodies", key))
                    reports.append(
                        {
                            "file": file.path,
                            "body": key,
                            "registration": node["id"],
                            "review": index,
                            "state": _review_state(evidence, matches),
                            "anchor_matches": matches,
                        }
                    )
    return reports


def _review_state(review: dict[str, JsonValue], matches: bool) -> str:
    if review.get("verdict") == "disputed":
        return "disputed"
    return "recorded" if matches else "stale"


def _review_pins(inputs: AuthorInputSet) -> dict[tuple[str, str], JsonValue]:
    pins: dict[tuple[str, str], JsonValue] = {}
    for file in inputs.anchor_files:
        tree = object_value(parse_document(file.raw).value)
        for area in ("cards", "bodies"):
            for key, value in object_value(tree[area]).items():
                pins[area, key] = object_value(value)["basis"]
    return pins


def _body_users(handwritten: Handwritten, name: str, edition: int) -> set[str]:
    return {
        number
        for number, value in handwritten.cards.items()
        if any(
            object_value(a).get("use") == name
            and object_value(a).get("edition", edition) == edition
            for a in array(value["abilities"])
        )
    }
