"""Resolve current author storage without persisted verification artifacts."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value, string
from sve_carddb.domains.effects.author_direct import direct
from sve_carddb.domains.effects.author_direct import materialize as materialize_direct
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
from sve_carddb.domains.effects.author_materialization import (
    MaterializationContext,
    materialize_card,
)
from sve_carddb.domains.effects.author_set import AuthorInputSet, InputFile
from sve_carddb.domains.effects.author_sources import SourceError

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_current import CurrentAuthorSources
    from sve_carddb.domains.effects.author_expansion import AuthorExpander

STORAGE_FORMAT = 2
SHARED_CARD_THRESHOLD = 3


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


@dataclass(frozen=True)
class Materialized:
    inputs: AuthorInputSet
    failures: dict[str, Diagnostic]


def _store_body(
    bodies: dict[tuple[str, int], JsonValue],
    name: str,
    edition: int,
    definition: JsonValue,
) -> None:
    key = name, edition
    if key in bodies and bodies[key] != definition:
        raise SourceError("body_source_frame_mismatch")
    bodies[key] = definition


def materialize_inputs(
    inputs: AuthorInputSet,
    current: CurrentAuthorSources,
    expander: AuthorExpander,
) -> Materialized:
    """Resolve current inputs on every read; no persisted fingerprints authorize authors."""
    handwritten = handwriting(inputs)
    cards: dict[str, JsonValue] = {}
    bodies: dict[tuple[str, int], JsonValue] = {}
    failures: dict[str, Diagnostic] = {}
    for record in inputs.index.records:
        if record.kind != "author_effect_set":
            continue
        number = record.record_key
        try:
            if direct(handwritten, number):
                cards[number] = materialize_direct(current, number, handwritten)
                continue
            payload = current.card_payload(current.printing_card(number), inspect=True)
            context = MaterializationContext(
                current, expander.schema_directory, payload
            )
            _, card, definitions, _ = materialize_card(
                context,
                number,
                handwritten,
                container=string(handwritten.cards[number].get("container", "faces")),
            )
            cards[number] = card
            uses = (
                object_value(v) for v in array(handwritten.cards[number]["abilities"])
            )
            names = (string(v["use"]) for v in uses if "use" in v)
            for name, (definition, edition) in zip(names, definitions, strict=True):
                _store_body(bodies, name, edition, definition)
        except (AuthorInputError, SourceError) as error:
            failure = (
                error.diagnostic
                if isinstance(error, AuthorInputError)
                else Diagnostic("source", error.reason)
            )
            failures[number] = replace(failure, card_no=number, card_id=record.card_id)
    authors = tuple(
        input_file(
            file.path,
            {
                "format": 1,
                "kind": "author_effect_set",
                "scope": "shared",
                "cards": {
                    number: cards[number]
                    for number in object_value(
                        object_value(parse_document(file.raw).value)["cards"]
                    )
                    if number in cards
                },
            },
            input_hash=file.input_hash,
        )
        for file in inputs.author_files
    )
    companions = tuple(
        input_file(
            file.path,
            {
                "format": 1,
                "kind": "effect_body_shard",
                "scope": "shared",
                "records": [
                    bodies[body.id, body.edition]
                    for body in decode(parse_document(file.raw), BodyFile).records
                    if (body.id, body.edition) in bodies
                ],
            },
        )
        for file in inputs.body_files
    )
    return Materialized(
        replace(inputs, author_files=authors, body_files=companions),
        failures,
    )


def review_states(inputs: AuthorInputSet) -> list[JsonValue]:
    """Report recorded verdicts without treating historical metadata as admission proof."""
    reports: list[JsonValue] = []
    for file in inputs.author_files:
        cards = object_value(object_value(parse_document(file.raw).value)["cards"])
        for number, value in cards.items():
            reviews = object_value(object_value(value)["meta"]).get("reviewed_by", [])
            for index, review in enumerate(array(reviews)):
                reports.append(
                    {
                        "file": file.path,
                        "card_no": number,
                        "review": index,
                        "state": object_value(review).get("verdict", "recorded"),
                    }
                )
    return reports


def body_usage(inputs: AuthorInputSet) -> list[JsonValue]:
    """Count distinct permanent game cards, so reprints never inflate the threshold."""
    handwritten = handwriting(inputs)
    identities = {r.record_key: r.card_id for r in inputs.index.records}
    reports: list[JsonValue] = []
    for (name, edition), body in handwritten.bodies.items():
        users = {
            identities[number]
            for number, card in handwritten.cards.items()
            if any(
                (value := object_value(a)).get("use") == name
                and value.get("edition", edition) == edition
                for a in array(card["abilities"])
            )
        }
        reports.append(
            {
                "body": name,
                "edition": edition,
                "cards": len(users),
                "recommendation": "shared"
                if len(users) >= SHARED_CARD_THRESHOLD
                else "light",
                "admission": body.admission,
            }
        )
    return reports
