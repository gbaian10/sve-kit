"""Keep every implemented author definition aligned with its Python decoder."""

from copy import deepcopy
from functools import cache
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts import author_leaves as author
from sve_carddb.contracts import author_references as references
from sve_carddb.contracts.four_layer import Code
from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.core.models import Text, UInt

from .author_ability_types import ABILITIES, ABILITY_NAMES
from .author_cost_cases import COST_NAMES, COSTS
from .author_effect_cases import EFFECT_NAMES, EFFECTS
from .author_entry_types import ENTRIES, ENTRY_NAMES
from .author_history_cases import HISTORY, HISTORY_NAMES
from .author_numeric_cases import NUMERIC, NUMERIC_DEPENDENCIES, NUMERIC_NAMES
from .author_parent_cases import PARENT_NAMES, PARENTS
from .author_receipt_cases import RECEIPT_NAMES, RECEIPTS
from .author_recipient_cases import RECIPIENT_NAMES, RECIPIENTS
from .author_selection_cases import SELECTIONS
from .author_selector_cases import COUNTS, SELECTOR_NAMES, SELECTORS
from .author_step_types import STEP_NAMES, STEPS

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
    "CardName": TypeAdapter(author.CardName),
    "Concept": TypeAdapter(author.Concept),
    "Title": TypeAdapter(author.Title, config=CONFIG),
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
    "A.turn-interval": TypeAdapter(references.TurnInterval),
    "A.until-during-next-turn": TypeAdapter(references.UntilDuringNextTurn),
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
    reference = node.get("$ref", node.get("$dynamicRef"))
    if isinstance(reference, str):
        # Never silently discard constraints attached beside a reference.
        assert node.keys() <= {
            "$ref",
            "$dynamicRef",
            "$dynamicAnchor",
            "$defs",
            "$schema",
            "$id",
            "title",
            "description",
        }
        anchors = {
            "urn:sve-kit:author-nodes:1.0:authorParam#authorParam": "A.param",
            "urn:sve-kit:author-nodes:1.0:authorGenerated#authorGenerated": "A.ability.generated",
        }
        if reference in anchors:
            resource, name = "urn:sve-kit:author-nodes:1.0", anchors[reference]
        else:
            resource, name = reference.split("#/$defs/")
        source = (
            (root if root.get("$id") == resource else NODES)
            if resource == "urn:sve-kit:author-nodes:1.0"
            else root
        )
        definition = object_value(source["$defs"])[name]
        if names is not None and name in names:
            return {"$ref": names[name]}
        return _shape(definition, source, names)
    shaped = {
        key: _shape(value, root, names)
        for key, value in node.items()
        if key
        not in {"$schema", "$id", "$dynamicAnchor", "$defs", "title", "description"}
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
    if (
        name in COUNTS
        or name in HISTORY
        or name
        in {"A.material-filter", "A.expr.count-of-filtered", "A.cond.did-filtered"}
    ):
        assert_selector_definition(schema, name)
        return
    python = adapter.json_schema()
    assert _shape(object_value(schema["$defs"])[name], schema) == _shape(python, python)


def _assert_complete_definitions() -> None:
    assert (
        object_value(NODES["$defs"]).keys() - {"expanded"}
        == (
            LEAVES
            | REFERENCES
            | NUMERIC
            | SELECTIONS
            | RECEIPTS
            | COUNTS
            | SELECTORS
            | RECIPIENTS
            | PARENTS
            | HISTORY
            | COSTS
            | EFFECTS
            | STEPS
            | ABILITIES
            | ENTRIES
        ).keys()
    )
    assert object_value(INDEX_SCHEMA["$defs"]).keys() == INDEX.keys()


@cache
def _python_schemas() -> dict[str, dict[str, JsonValue]]:
    # Generate the shared recursive graph once rather than once per root.
    adapters = (
        SELECTORS
        | RECIPIENTS
        | PARENTS
        | COUNTS
        | NUMERIC
        | RECEIPTS
        | HISTORY
        | COSTS
        | EFFECTS
        | STEPS
        | ABILITIES
        | ENTRIES
    )
    roots, graph = TypeAdapter.json_schemas(
        [(name, "validation", adapter) for name, adapter in adapters.items()]
    )
    return {
        name: {**root, "$defs": graph["$defs"]} for (name, _), root in roots.items()
    }


def assert_selector_definition(schema: dict[str, JsonValue], name: str) -> None:
    python = _python_schemas()[name]
    names = (
        SELECTOR_NAMES
        | RECIPIENT_NAMES
        | NUMERIC_NAMES
        | NUMERIC_DEPENDENCIES
        | PARENT_NAMES
        | HISTORY_NAMES
        | RECEIPT_NAMES
        | COST_NAMES
        | EFFECT_NAMES
        | STEP_NAMES
        | ABILITY_NAMES
        | ENTRY_NAMES
    )
    python_names = {value: key for key, value in names.items()}
    # Expand the root once; only internal recursive references preserve names.
    reference = python.get("$ref")
    body = (
        object_value(python["$defs"])[reference.removeprefix("#/$defs/")]
        if isinstance(reference, str)
        else python
    )
    assert _shape(
        object_value(schema["$defs"])[name],
        schema,
        {key: key for key in names if name != "authored" or key != "A.file"},
    ) == _shape(body, python, python_names)


def _assert_index_envelope() -> None:
    python = author.EffectsIndex.model_json_schema()
    assert _shape(INDEX_SCHEMA, INDEX_SCHEMA) == _shape(python, python)


def test_schema_and_python_types_stay_aligned() -> None:
    _assert_complete_definitions()
    _assert_index_envelope()
    for name, adapter in (LEAVES | REFERENCES | SELECTIONS | RECEIPTS).items():
        _assert_definition(NODES, name, adapter)
    for name in (
        COUNTS
        | SELECTORS
        | RECIPIENTS
        | PARENTS
        | HISTORY
        | COSTS
        | EFFECTS
        | NUMERIC
        | STEPS
        | ABILITIES
        | ENTRIES
    ):
        assert_selector_definition(NODES, name)
    for name, adapter in INDEX.items():
        _assert_definition(INDEX_SCHEMA, name, adapter)


def test_schema_alignment_detects_an_open_object() -> None:
    changed = deepcopy(NODES)
    definition = object_value(object_value(changed["$defs"])["IntParamValue"])
    definition["additionalProperties"] = True
    with pytest.raises(AssertionError):
        _assert_definition(changed, "IntParamValue", LEAVES["IntParamValue"])


def _local_author_references(node: JsonValue) -> JsonValue:
    if isinstance(node, list):
        return [_local_author_references(value) for value in node]
    if not isinstance(node, dict):
        return node
    return {
        key: value.removeprefix("urn:sve-kit:author-nodes:1.0")
        if key == "$ref" and isinstance(value, str)
        else _local_author_references(value)
        for key, value in node.items()
    }


def test_storage_ability_heads_follow_the_authority() -> None:
    storage = object_value(parse((SCHEMAS / "author-storage.schema.json").read_bytes()))
    cards = object_value(object_value(storage["properties"])["cards"])
    card = object_value(cards["additionalProperties"])
    abilities = object_value(object_value(card["properties"])["abilities"])
    variants = array(object_value(abilities["items"])["oneOf"])
    stored = {}
    for value in variants:
        variant = object_value(value)
        heads = {string(name) for name in array(variant["required"])} - {"line"}
        assert len(heads) == 1
        (head,) = heads
        assert head not in stored
        assert variant["additionalProperties"] is False
        stored[head] = object_value(variant["properties"])
    definitions = object_value(NODES["$defs"])
    assert stored.keys() == object_value(
        object_value(definitions["A.ability-head"])["properties"]
    ).keys() | {"use"}
    for reference in array(object_value(definitions["A.inline-ability"])["anyOf"]):
        name = string(object_value(reference)["$ref"]).removeprefix("#/$defs/")
        variant = object_value(definitions[name])
        heads = {string(key) for key in array(variant["required"])} - {
            "line",
            "sources",
            "ports",
        }
        assert len(heads) == 1
        (head,) = heads
        expected = {
            key: value
            for key, value in object_value(variant["properties"]).items()
            if key not in {"line", "section", "sources"}
        }
        actual = {
            key: value
            for key, value in stored[head].items()
            if key not in {"line", "face", "section", "scope", "admission", "covers"}
        }
        assert _local_author_references(actual) == _local_author_references(expected)
