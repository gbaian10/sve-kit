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

from .author_numeric_cases import NUMERIC, NUMERIC_DEPENDENCIES, NUMERIC_NAMES
from .author_selection_cases import SELECTIONS

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
    "A.leader-of": TypeAdapter(references.LeaderOf),
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


def _shape(
    node: JsonValue,
    root: dict[str, JsonValue],
    names: dict[str, str] | None = None,
) -> JsonValue:
    if isinstance(node, list):
        return [_shape(child, root, names) for child in node]
    if not isinstance(node, dict):
        return node
    reference = node.get("$ref")
    if isinstance(reference, str):
        resource, name = reference.split("#/$defs/")
        source = NODES if resource == "urn:sve-kit:author-nodes:1.0" else root
        definition = object_value(source["$defs"])[name]
        if names is not None and name in names:
            return {"$ref": names[name]}
        return _shape(definition, source, names)
    shaped = {
        key: _shape(value, root, names)
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
    assert (
        object_value(NODES["$defs"]).keys()
        == (LEAVES | REFERENCES | NUMERIC | SELECTIONS).keys()
    )
    assert object_value(INDEX_SCHEMA["$defs"]).keys() == INDEX.keys()


@pytest.mark.parametrize("name", LEAVES)
def test_leaf_type_drift(name: str) -> None:
    _assert_definition(NODES, name, LEAVES[name])


@pytest.mark.parametrize("name", REFERENCES)
def test_reference_type_drift(name: str) -> None:
    _assert_definition(NODES, name, REFERENCES[name])


@pytest.mark.parametrize("name", SELECTIONS)
def test_selection_type_drift(name: str) -> None:
    _assert_definition(NODES, name, SELECTIONS[name])


@pytest.mark.parametrize("name", ["A.target.role-one", "A.leader-of"])
@pytest.mark.parametrize("mutation", ["required", "discriminator", "exclusive"])
def test_selection_mapping_drift_detects_variant_changes(
    name: str, mutation: str
) -> None:
    changed = deepcopy(NODES)
    definition = object_value(object_value(changed["$defs"])[name])
    properties = object_value(definition["properties"])
    if mutation == "required":
        definition["required"] = []
    elif mutation == "discriminator":
        value = properties.pop(next(iter(properties)))
        properties["wrong_key"] = value
        definition["required"] = ["wrong_key"]
    else:
        properties["other_variant"] = {"type": "boolean"}
    with pytest.raises(AssertionError):
        _assert_definition(changed, name, (REFERENCES | SELECTIONS)[name])


@pytest.mark.parametrize("mutation", ["empty", "key_domain", "value_type"])
def test_role_map_drift_detects_declaration_changes(mutation: str) -> None:
    changed = deepcopy(NODES)
    definition = object_value(object_value(changed["$defs"])["A.target.roles"])
    if mutation == "empty":
        definition["minProperties"] = 0
    elif mutation == "key_domain":
        definition["propertyNames"] = {"type": "string"}
    else:
        definition["additionalProperties"] = {"type": "integer"}
    with pytest.raises(AssertionError):
        _assert_definition(changed, "A.target.roles", SELECTIONS["A.target.roles"])


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


def _assert_numeric_names(schema: dict[str, JsonValue], names: dict[str, str]) -> None:
    assert names.keys() == NUMERIC.keys()
    combined = names | NUMERIC_DEPENDENCIES
    assert len(set(combined.values())) == len(combined)
    assert combined.keys() <= object_value(schema["$defs"]).keys()
    for name, adapter in NUMERIC.items():
        python = adapter.json_schema()
        reference = python.get("$ref")
        actual = (
            reference.removeprefix("#/$defs/")
            if isinstance(reference, str)
            else python["title"]
        )
        assert actual == names[name]


def _assert_numeric_definition(schema: dict[str, JsonValue], name: str) -> None:
    python = NUMERIC[name].json_schema()
    reference = python.get("$ref")
    body = (
        object_value(python["$defs"])[reference.removeprefix("#/$defs/")]
        if isinstance(reference, str)
        else python
    )
    names = NUMERIC_NAMES | NUMERIC_DEPENDENCIES
    schema_names = {key: key for key in names}
    python_names = {value: key for key, value in names.items()}
    # Every named node is compared separately, so cycles preserve target identity.
    assert _shape(object_value(schema["$defs"])[name], schema, schema_names) == _shape(
        body, python, python_names
    )


def test_numeric_name_correspondence_is_bijective() -> None:
    _assert_numeric_names(NODES, NUMERIC_NAMES)


@pytest.mark.parametrize("mutation", ["missing_definition", "duplicate_name"])
def test_numeric_name_check_detects_incomplete_correspondence(mutation: str) -> None:
    changed = deepcopy(NODES)
    names = NUMERIC_NAMES.copy()
    if mutation == "missing_definition":
        object_value(changed["$defs"]).pop("A.expr.plus")
    else:
        names["A.expr.plus"] = names["A.expr.minus"]
    with pytest.raises(AssertionError):
        _assert_numeric_names(changed, names)


@pytest.mark.parametrize("name", NUMERIC)
def test_numeric_type_drift(name: str) -> None:
    _assert_numeric_definition(NODES, name)


@pytest.mark.parametrize(
    "mutation",
    [
        "required",
        "discriminator",
        "plus_two_params",
        "minus_first_param",
        "times_param",
        "enum",
        "unit_domain",
        "recursive_target",
    ],
)
def test_numeric_drift_detects_variant_changes(mutation: str) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    param: JsonValue = {"$ref": "#/$defs/A.param"}
    if mutation == "required":
        name = "A.expr.resource-payload"
        object_value(definitions[name])["required"] = ["who"]
    elif mutation == "discriminator":
        name = "A.expr.plus"
        definition = object_value(definitions[name])
        props = object_value(definition["properties"])
        props["add"] = props.pop("plus")
        definition["required"] = ["add"]
    elif mutation in {"plus_two_params", "recursive_target"}:
        name = "A.expr.plus"
        props = object_value(object_value(definitions[name])["properties"])
        variants = object_value(props["plus"])["anyOf"]
        assert isinstance(variants, list)
        first = object_value(variants[0])
        if mutation == "plus_two_params":
            additional = deepcopy(first)
            additional["prefixItems"] = [param, param]
            variants.append(additional)
        else:
            operands = first["prefixItems"]
            assert isinstance(operands, list)
            operands[0] = {"$ref": "#/$defs/A.cond.numeric-core"}
    elif mutation in {"minus_first_param", "times_param"}:
        key = "minus" if mutation == "minus_first_param" else "times"
        name = "A.expr." + key
        props = object_value(object_value(definitions[name])["properties"])
        operands = object_value(props[key])["prefixItems"]
        assert isinstance(operands, list)
        operands[0] = {"anyOf": [operands[0], param]}
    elif mutation == "enum":
        name = "A.cond.cmp-payload"
        props = object_value(object_value(definitions[name])["properties"])
        values = object_value(props["op"])["enum"]
        assert isinstance(values, list)
        values.append("contains")
    else:
        name = "A.expr.per-payload"
        props = object_value(object_value(definitions[name])["properties"])
        object_value(props["unit"])["minimum"] = 0
    with pytest.raises(AssertionError):
        _assert_numeric_definition(changed, name)


def test_numeric_drift_rejects_dangling_references() -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    props = object_value(object_value(definitions["A.expr.half-up"])["properties"])
    props["half_up"] = {"$ref": "#/$defs/A.expr.missing"}
    with pytest.raises(KeyError):
        _assert_numeric_definition(changed, "A.expr.half-up")
