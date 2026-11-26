"""Read shared author body and review metadata from immutable shards."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, object_value
from sve_carddb.domains.effects.author_handwritten import (
    BodyFile,
    Handwritten,
    MainFile,
    decode,
)
from sve_carddb.domains.effects.author_inputs import parse_document

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_set import AuthorInputSet

SHARED_CARD_THRESHOLD = 3


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
