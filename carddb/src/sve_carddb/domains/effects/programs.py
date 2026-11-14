"""Project every admitted author, including light admission, into public DSL programs."""

from typing import TYPE_CHECKING

from sve_carddb.build import Json
from sve_carddb.contracts.profiles import PROGRAMS
from sve_carddb.contracts.snapshot import validate
from sve_carddb.core.json import array, canonical, digest, object_value, string
from sve_carddb.domains.effects.author_current import CardRejected
from sve_carddb.domains.effects.author_inputs import AuthorInputError

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.effects.author_current import (
        CardAccepted,
        CurrentAuthorSources,
    )
    from sve_carddb.domains.effects.author_expansion import AuthorExpander
    from sve_carddb.domains.effects.author_set import AuthorInputSet


def public_program(result: CardAccepted) -> dict[str, JsonValue]:
    """Keep executable nodes and permanent identities without author/source audit data."""
    tree = object_value(result.candidate.tree)
    card = object_value(object_value(tree["cards"])[result.record.record_key])
    program: dict[str, JsonValue] = {
        "card_id": result.record.card_id,
        "scope": result.record.scope,
    }
    for key in ("abilities", "faces", "deck_rule", "token"):
        if key in card:
            program[key] = card[key]
    if "abilities" in program:
        program["abilities"] = [
            _ability(object_value(a)) for a in array(program["abilities"])
        ]
    if "faces" in program:
        program["faces"] = {
            face: {
                "abilities": [
                    _ability(object_value(a))
                    for a in array(object_value(value)["abilities"])
                ]
            }
            for face, value in object_value(program["faces"]).items()
        }
    validate("DSLProgram", program, PROGRAMS)
    return program


def _ability(value: dict[str, JsonValue]) -> JsonValue:
    return {key: item for key, item in value.items() if key != "sources"}


def populate(
    db: Database,
    current: CurrentAuthorSources,
    inputs: AuthorInputSet,
    expander: AuthorExpander,
) -> None:
    """Reject incomplete admission rather than silently publishing a partial program set."""
    results = current.validate_inputs(inputs, expander)
    documents: dict[str, tuple[str, str, str]] = {}
    for result in results:
        if isinstance(result, CardRejected):
            raise AuthorInputError(result.diagnostic)
        program = public_program(result)
        hashed = digest(canonical(program))
        identifier = "dsl:" + hashed[7:]
        authored = object_value(
            object_value(object_value(result.candidate.tree)["cards"])[
                result.record.record_key
            ]
        )
        status = string(object_value(authored["meta"]).get("status", "draft"))
        support_status = "reviewed" if status in {"reviewed", "verified"} else "draft"
        documents[result.record.card_id] = identifier, hashed, support_status
        db.insert(
            "dsl_document",
            {
                "id": "dsl:" + hashed[7:],
                "card_id": result.record.card_id,
                "scope": result.record.scope,
                "dsl_version": "1.0",
                "status": status,
                "ast": Json(program),
                "program_hash": hashed,
            },
        )

    existing = {
        (row.values["card_id"], row.values["region"])
        for row in db.rows("card_engine_support")
    }
    regional: set[tuple[str, str]] = set()
    for row in db.rows("printing"):
        card_id, region = row.values["card_id"], row.values["region"]
        assert isinstance(card_id, str)
        assert isinstance(region, str)
        regional.add((card_id, region))
    for card_id, region in sorted(regional):
        document = documents.get(card_id)
        values: dict[str, Value] = {
            "card_id": card_id,
            "region": region,
            "status": "missing_dsl" if document is None else document[2],
            "dsl_id": None if document is None else document[0],
            "candidate_hash": None if document is None else document[1],
            "dsl_version": None if document is None else "1.0",
            "validation_state": "not_applicable" if document is None else "fresh",
            "reason_codes": Json(
                ["missing_dsl" if document is None else "engine_unassigned"]
            ),
            "automatic": False,
        }
        if (card_id, region) in existing:
            if document is not None:
                db.update(
                    "card_engine_support",
                    {"card_id": card_id, "region": region},
                    values,
                )
        else:
            db.insert("card_engine_support", values)
