"""Receipt reads preserve projection, value categories and closed material fields."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import TypeAdapter, ValidationError

from sve_carddb.contracts import author_references as refs
from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import NUMERIC, C, E
from .author_receipt_cases import (
    CONFIG,
    COUNTED,
    DID,
    RECEIPT_CASES,
    RECEIPTS,
    SNAPSHOT,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMA_PATH = Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"
ADAPTERS = (
    RECEIPTS
    | NUMERIC
    | {
        "A.ref": TypeAdapter(refs.Ref, config=CONFIG),
        "A.player-ref": TypeAdapter(refs.PlayerRef, config=CONFIG),
        "A.leader-recipient": TypeAdapter(refs.LeaderRecipient, config=CONFIG),
    }
)


@pytest.fixture(scope="module")
def schema() -> dict[str, JsonValue]:
    result = object_value(parse(SCHEMA_PATH.read_bytes()))
    Draft202012Validator.check_schema(result)
    return result


def _check(
    schema: dict[str, JsonValue],
    name: str,
    value: JsonValue,
    *,
    accepted: bool,
) -> None:
    validator = Draft202012Validator(
        {"$id": schema["$id"], "$defs": schema["$defs"], "$ref": "#/$defs/" + name}
    )
    assert validator.is_valid(value) == accepted
    adapter = ADAPTERS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value), strict=True)
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value), strict=True)


@pytest.mark.parametrize(("name", "value"), RECEIPT_CASES)
def test_receipt_nodes_round_trip(
    schema: dict[str, JsonValue], name: str, value: JsonValue
) -> None:
    _check(schema, name, value, accepted=True)
    assert not Draft202012Validator(schema).is_valid(value)


@pytest.mark.parametrize(
    "where",
    [{}, None],
)
def test_material_filter_does_not_select_live_sources(
    schema: dict[str, JsonValue], where: JsonValue
) -> None:
    _check(schema, "A.material-filter", where, accepted=False)
    _check(schema, E, {"count_of": {**COUNTED, "where": where}}, accepted=False)
    _check(schema, C, {"did": {**DID, "where": where}}, accepted=False)


@pytest.mark.parametrize(
    "receipt", ["revealed", {"event": "move_receipt"}, {"entry_payment": "additional"}]
)
def test_receipt_sources_do_not_infer_binding_authority(
    schema: dict[str, JsonValue], receipt: JsonValue
) -> None:
    _check(schema, E, {"count_of": receipt}, accepted=True)
    _check(schema, C, {"did": receipt}, accepted=True)
    _check(schema, E, {"count_of": {**COUNTED, "receipt": receipt}}, accepted=True)
    _check(schema, C, {"did": {**DID, "result": receipt}}, accepted=True)
    for view in ("before", "after"):
        _check(
            schema,
            "A.snapshot-read",
            {"objects_of": receipt, "view": view},
            accepted=True,
        )


@pytest.mark.parametrize(
    "value",
    [SNAPSHOT, {"same_as": SNAPSHOT}, {"lki_of": "this"}],
)
@pytest.mark.parametrize("name", [E, C, "A.ref", "A.player-ref", "A.leader-recipient"])
def test_snapshots_do_not_grant_numeric_or_operation_authority(
    schema: dict[str, JsonValue], value: JsonValue, name: str
) -> None:
    _check(schema, name, value, accepted=False)
