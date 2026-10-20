"""Keep every implemented author definition aligned with its Python decoder."""

from copy import deepcopy
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts import author_leaves as author
from sve_carddb.contracts import author_references as references
from sve_carddb.contracts.four_layer import Code
from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.core.models import Text, UInt

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMAS = Path(__file__).resolve().parents[3] / "dsl"
CONFIG = ConfigDict(regex_engine="python-re")
LEAVES: dict[str, TypeAdapter[object]] = {
    "Int": TypeAdapter(author.Int, config=CONFIG),
    "Nat": TypeAdapter(UInt, config=CONFIG),
    "Code": TypeAdapter(Code, config=CONFIG),
    "ParamName": TypeAdapter(author.ParamName, config=CONFIG),
    "CardNumber": TypeAdapter(Text),
    "CardName": TypeAdapter(Text),
    "Concept": TypeAdapter(Text),
    "CardKind": TypeAdapter(Code, config=CONFIG),
    "Player": TypeAdapter(author.Player),
    "Zone": TypeAdapter(author.Zone),
    "Phase": TypeAdapter(author.Phase),
    "ParamType": TypeAdapter(author.ParamType),
    "ParamSource": TypeAdapter(author.ParamSource),
    "ParamDecl": TypeAdapter(author.ParamDecl),
    "A.param": TypeAdapter(author.ParamRef),
    "IntParamValue": TypeAdapter(author.IntParamValue),
    "NatParamValue": TypeAdapter(author.NatParamValue),
    "CardNameParamValue": TypeAdapter(author.CardNameParamValue),
    "ConceptParamValue": TypeAdapter(author.ConceptParamValue),
    "CardKindParamValue": TypeAdapter(author.CardKindParamValue),
    "PlayerParamValue": TypeAdapter(author.PlayerParamValue),
    "ZoneParamValue": TypeAdapter(author.ZoneParamValue),
    "PhaseParamValue": TypeAdapter(author.PhaseParamValue),
    "ParamValue": TypeAdapter(author.ParamValue),
    "ParamDeclarations": TypeAdapter(author.ParamDeclarations, config=CONFIG),
    "ParamValues": TypeAdapter(author.ParamValues, config=CONFIG),
}
REFERENCES: dict[str, TypeAdapter[object]] = {
    "BindingName": TypeAdapter(references.BindingName, config=CONFIG),
    "RoleName": TypeAdapter(references.RoleName, config=CONFIG),
    "ReservedRefName": TypeAdapter(references.ReservedRefName),
    "ReservedPlayerName": TypeAdapter(references.ReservedPlayerName),
    "PlayerSetName": TypeAdapter(references.PlayerSetName),
    "Edge": TypeAdapter(references.Edge),
    "A.event-move-receipt": TypeAdapter(references.EventMoveReceipt),
    "A.entry-payment-ref": TypeAdapter(references.EntryPaymentRef),
    "A.follow-source": TypeAdapter(references.FollowSource, config=CONFIG),
    "A.receipt-ref": TypeAdapter(references.ReceiptRef, config=CONFIG),
    "A.follow-ref": TypeAdapter(references.FollowRef),
    "A.role-selection": TypeAdapter(references.RoleSelection),
    "A.role-ref": TypeAdapter(references.RoleRef),
    "A.event-object-ref": TypeAdapter(references.EventObjectRef),
    "A.ref": TypeAdapter(references.Ref, config=CONFIG),
    "A.owner-ref": TypeAdapter(references.OwnerRef),
    "A.master-ref": TypeAdapter(references.MasterRef),
    "A.player-ref": TypeAdapter(references.PlayerRef, config=CONFIG),
    "A.player-set": TypeAdapter(references.PlayerSet, config=CONFIG),
    "A.single-leader": TypeAdapter(references.SingleLeader),
    "A.multiple-leaders": TypeAdapter(references.MultipleLeaders),
    "A.leader-recipient": TypeAdapter(references.LeaderRecipient, config=CONFIG),
    "A.phase-edge": TypeAdapter(references.PhaseEdge),
    "A.turn-edge": TypeAdapter(references.TurnEdge),
    "A.phase-interval": TypeAdapter(references.PhaseInterval),
    "A.until-next-phase": TypeAdapter(references.UntilNextPhase),
    "A.until-next-turn": TypeAdapter(references.UntilNextTurn),
    "A.until-during-next-phase": TypeAdapter(references.UntilDuringNextPhase),
    "A.until": TypeAdapter(references.Until, config=CONFIG),
}
INDEX: dict[str, TypeAdapter[object]] = {
    "CardId": TypeAdapter(author.CardId, config=CONFIG),
    "RelativeFile": TypeAdapter(author.RelativeFile, config=CONFIG),
    "Scope": TypeAdapter(author.Scope),
    "PrototypeIndexRecord": TypeAdapter(author.PrototypeIndexRecord),
    "AuthorIndexRecord": TypeAdapter(author.AuthorIndexRecord),
    "IndexRecord": TypeAdapter(author.IndexRecord),
}
NODES = object_value(parse((SCHEMAS / "author-nodes.schema.json").read_bytes()))
INDEX_SCHEMA = object_value(parse((SCHEMAS / "effects-index.schema.json").read_bytes()))


def _shape(node: JsonValue, root: dict[str, JsonValue]) -> JsonValue:
    if isinstance(node, list):
        return [_shape(child, root) for child in node]
    if not isinstance(node, dict):
        return node
    reference = node.get("$ref")
    if isinstance(reference, str):
        resource, name = reference.split("#/$defs/")
        source = NODES if resource == "urn:sve-kit:author-nodes:1.0" else root
        return _shape(object_value(source["$defs"])[name], source)
    shaped = {
        key: _shape(value, root)
        for key, value in node.items()
        if key not in {"$schema", "$id", "$defs", "title", "description"}
    }
    for key in ("required", "enum"):
        value = shaped.get(key)
        if isinstance(value, list):
            shaped[key] = sorted(value, key=canonical)
    # Acceptance tests separately check the disjoint literal and name-pattern domains.
    union = shaped.pop("anyOf", shaped.pop("oneOf", None))
    if isinstance(union, list):
        shaped["union"] = sorted(union, key=canonical)
    # Pydantic emits patternProperties for validated dict key annotations.
    patterns = shaped.get("patternProperties")
    if isinstance(patterns, dict):
        assert len(patterns) == 1
        pattern, value = next(iter(patterns.items()))
        shaped.pop("patternProperties")
        shaped["propertyNames"] = {"type": "string", "pattern": pattern}
        shaped["additionalProperties"] = value
    return shaped


def _assert_definition(
    schema: dict[str, JsonValue], name: str, adapter: TypeAdapter[object]
) -> None:
    python = adapter.json_schema()
    assert _shape(object_value(schema["$defs"])[name], schema) == _shape(python, python)


def test_every_implemented_definition_has_a_python_type() -> None:
    assert object_value(NODES["$defs"]).keys() == (LEAVES | REFERENCES).keys()
    assert object_value(INDEX_SCHEMA["$defs"]).keys() == INDEX.keys()


@pytest.mark.parametrize("name", LEAVES)
def test_leaf_type_drift(name: str) -> None:
    _assert_definition(NODES, name, LEAVES[name])


@pytest.mark.parametrize("name", REFERENCES)
def test_reference_type_drift(name: str) -> None:
    _assert_definition(NODES, name, REFERENCES[name])


@pytest.mark.parametrize("name", INDEX)
def test_index_type_drift(name: str) -> None:
    _assert_definition(INDEX_SCHEMA, name, INDEX[name])


def test_index_envelope_drift() -> None:
    python = author.EffectsIndex.model_json_schema()
    assert _shape(INDEX_SCHEMA, INDEX_SCHEMA) == _shape(python, python)


@pytest.mark.parametrize(
    "mutation",
    ["unknown_key", "optional_key", "integer_domain", "wrong_scalar", "open_object"],
)
def test_drift_check_detects_incompatible_contract_changes(mutation: str) -> None:
    changed = deepcopy(NODES)
    definition = object_value(object_value(changed["$defs"])["IntParamValue"])
    properties = object_value(definition["properties"])
    if mutation == "unknown_key":
        properties["default"] = {"type": "integer"}
    elif mutation == "optional_key":
        definition["required"] = ["type"]
    elif mutation == "integer_domain":
        properties["value"] = {"type": "integer", "minimum": 0}
    elif mutation == "wrong_scalar":
        properties["value"] = {"type": "boolean"}
    else:
        definition["additionalProperties"] = True
    with pytest.raises(AssertionError):
        _assert_definition(changed, "IntParamValue", LEAVES["IntParamValue"])


@pytest.mark.parametrize(
    "mutation",
    ["required", "discriminant", "exclusive", "open_object", "enum", "reserved_name"],
)
def test_reference_drift_detects_variant_changes(mutation: str) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    if mutation == "required":
        name = "A.phase-edge"
        object_value(definitions[name])["required"] = ["who", "phase"]
    elif mutation == "discriminant":
        name = "A.event-object-ref"
        props = object_value(object_value(definitions[name])["properties"])
        props["event"] = {"type": "string", "const": "move_receipt"}
    elif mutation == "exclusive":
        name = "A.owner-ref"
        props = object_value(object_value(definitions[name])["properties"])
        props["master_of"] = {"$ref": "#/$defs/A.ref"}
    elif mutation == "open_object":
        name = "A.role-ref"
        object_value(definitions[name])["additionalProperties"] = True
    elif mutation == "enum":
        name = "ReservedPlayerName"
        object_value(definitions[name])["enum"] = ["self", "opp", "opponents"]
    else:
        name = "BindingName"
        pattern = object_value(definitions[name])["pattern"]
        assert isinstance(pattern, str)
        object_value(definitions[name])["pattern"] = pattern.replace("|opponents", "")
    with pytest.raises(AssertionError):
        _assert_definition(changed, name, REFERENCES[name])
