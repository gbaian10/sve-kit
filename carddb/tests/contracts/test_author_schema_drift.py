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
    reference = node.get("$ref")
    if isinstance(reference, str):
        # Never silently discard constraints attached beside a reference.
        assert node.keys() <= {
            "$ref",
            "$defs",
            "$schema",
            "$id",
            "title",
            "description",
        }
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


def test_every_implemented_definition_has_a_python_type() -> None:
    assert (
        object_value(NODES["$defs"]).keys()
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


@pytest.mark.parametrize("name", LEAVES)
def test_leaf_type_drift(name: str) -> None:
    _assert_definition(NODES, name, LEAVES[name])


@pytest.mark.parametrize("name", REFERENCES)
def test_reference_type_drift(name: str) -> None:
    _assert_definition(NODES, name, REFERENCES[name])


@pytest.mark.parametrize("name", SELECTIONS)
def test_selection_type_drift(name: str) -> None:
    _assert_definition(NODES, name, SELECTIONS[name])


@pytest.mark.parametrize("name", RECEIPTS)
def test_receipt_type_drift(name: str) -> None:
    _assert_definition(NODES, name, RECEIPTS[name])


@pytest.mark.parametrize("name", COUNTS)
def test_count_query_type_drift(name: str) -> None:
    _assert_definition(NODES, name, COUNTS[name])


def assert_selector_definition(schema: dict[str, JsonValue], name: str) -> None:
    python = (
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
    )[name].json_schema()
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


@pytest.mark.parametrize(
    "name", SELECTORS | RECIPIENTS | PARENTS | HISTORY | COSTS | EFFECTS
)
def test_selector_type_drift(name: str) -> None:
    assert_selector_definition(NODES, name)


@pytest.mark.parametrize(
    "name",
    [
        "A.deck-slice",
        "A.target.exact",
        "A.target.up-to",
        "A.target.any",
        "A.act.choose.exact",
        "A.act.choose.up-to",
        "A.act.choose.any",
        "A.act.search.exact",
        "A.act.search.up-to",
        "A.cost.counter.self.exact",
        "A.cost.counter.self.up-to",
        "A.cost.counter.self.any",
        "A.cost.counter.selected.exact",
        "A.cost.counter.selected.up-to",
        "A.cost.counter.selected.any",
    ],
)
@pytest.mark.parametrize("mutation", ["required", "extra", "tag"])
def test_parent_drift_detects_variant_changes(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    if mutation == "required":
        node["required"] = []
    elif mutation == "extra":
        node["additionalProperties"] = True
    else:
        props = object_value(node["properties"])
        key = next(key for key in ("n", "upto", "any") if key in props)
        props["wrong_key"] = props.pop(key)
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "name",
    [
        "A.quantity",
        "A.selection-quantity",
        "A.target",
        "A.act.choose",
        "A.act.search",
        "A.cost.counter",
        "A.cost.counter.self",
        "A.cost.counter.selected",
    ],
)
@pytest.mark.parametrize("mutation", ["missing", "duplicate", "domain"])
def test_parent_drift_detects_union_changes(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    branches = object_value(object_value(changed["$defs"])[name])["anyOf"]
    assert isinstance(branches, list)
    if mutation == "missing":
        branches.pop()
    elif mutation == "duplicate":
        branches.append(deepcopy(branches[0]))
    else:
        branches[0] = {"$ref": "#/$defs/Int"}
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "mutation",
    ["holder_param", "self_holders", "distinct", "any", "source", "deck_players"],
)
def test_parent_drift_detects_position_boundaries(mutation: str) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    name = "A.target.exact"
    if mutation == "distinct":
        object_value(definitions[name]).pop("not")
    else:
        changes: dict[str, tuple[str, str, JsonValue]] = {
            "holder_param": (
                "A.cost.counter.selected.exact",
                "holders",
                {"$ref": "#/$defs/A.selection-quantity"},
            ),
            "self_holders": (
                "A.cost.counter.self.exact",
                "holders",
                {"$ref": "#/$defs/A.quantity"},
            ),
            "any": ("A.target.any", "any", {"type": "boolean"}),
            "source": (
                "A.act.choose.exact",
                "from",
                {"$ref": "#/$defs/A.sel.recipient"},
            ),
            "deck_players": (
                "A.deck-slice",
                "who",
                {"$ref": "#/$defs/A.player-ref"},
            ),
        }
        name, key, value = changes[mutation]
        object_value(object_value(definitions[name])["properties"])[key] = value
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "name", ["A.sel.leader-or", "A.sel.object-union", "A.sel.recipient-union"]
)
@pytest.mark.parametrize("mutation", ["required", "tag", "extra"])
def test_recipient_drift_detects_variant_changes(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    props = object_value(node["properties"])
    if mutation == "required":
        node["required"] = []
    elif mutation == "tag":
        key = "leader_or" if name == "A.sel.leader-or" else "any_of"
        props["wrong_key"] = props.pop(key)
    else:
        node["additionalProperties"] = True
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "mutation",
    ["contains", "contains_location", "minItems", "items", "true", "zone", "except"],
)
def test_recipient_drift_detects_domain_changes(mutation: str) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    name = "A.sel.recipient-union"
    node = object_value(definitions[name])
    array = object_value(object_value(node["properties"])["any_of"])
    if mutation in {"contains", "contains_location"}:
        condition = array.pop("contains")
        if mutation == "contains_location":
            node["contains"] = condition
    elif mutation == "minItems":
        array["minItems"] = 0
    elif mutation == "items":
        array["items"] = {"$ref": "#/$defs/A.sel.recipient"}
    elif mutation == "except":
        name = "A.sel.zone-objects"
        props = object_value(object_value(definitions[name])["properties"])
        props["except"] = {"$ref": "#/$defs/A.sel.recipient"}
    else:
        name = "A.sel.leader-or"
        props = object_value(object_value(definitions[name])["properties"])
        props["leader_or" if mutation == "true" else "zone"] = (
            {"type": "boolean"} if mutation == "true" else {"$ref": "#/$defs/Zone"}
        )
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "name",
    [
        "A.sel.object-atom",
        "A.sel.object-core",
        "A.sel.recipient-atom",
        "A.sel.recipient",
    ],
)
@pytest.mark.parametrize("mutation", ["duplicate", "missing"])
def test_recipient_drift_detects_sum_branch_changes(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    branches = object_value(object_value(changed["$defs"])[name])["anyOf"]
    assert isinstance(branches, list)
    if mutation == "duplicate":
        branches.append(deepcopy(branches[0]))
    else:
        branches.pop()
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "name",
    [
        "A.sel.zone-objects",
        "A.sel.in-objects",
        "A.expr.count-objects",
        "A.expr.count-union",
    ],
)
@pytest.mark.parametrize("mutation", ["required", "discriminator", "exclusive"])
def test_query_drift_detects_variant_changes(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    props = object_value(node["properties"])
    required = node["required"]
    assert isinstance(required, list)
    if mutation == "required":
        node["required"] = []
    elif mutation == "discriminator":
        key = required[0]
        assert isinstance(key, str)
        props["wrong_key"] = props.pop(key)
        node["required"] = ["wrong_key", *required[1:]]
    else:
        props["leader"] = {"type": "string", "const": "self"}
    if name in SELECTORS:
        with pytest.raises(AssertionError):
            assert_selector_definition(changed, name)
    else:
        with pytest.raises(AssertionError):
            _assert_definition(changed, name, COUNTS[name])


@pytest.mark.parametrize("name", ["A.sel.zone-objects", "A.sel.in-objects"])
@pytest.mark.parametrize(
    "mutation", ["scope_dependency", "filter_exclusion", "except_type"]
)
def test_selector_drift_detects_dependencies(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    if mutation == "scope_dependency":
        node.pop("dependentRequired")
    elif mutation == "filter_exclusion":
        node.pop("not")
    else:
        object_value(node["properties"])["except"] = {"$ref": "#/$defs/A.ref"}
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize("field", ["kind", "class", "trait", "traits_all", "name"])
def test_count_drift_detects_unregistered_attribute_parameters(field: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])["A.expr.count-objects"])
    properties = object_value(node["properties"])
    param: JsonValue = {"$ref": "#/$defs/A.param"}
    if field == "traits_all":
        sequence = object_value(properties[field])
        sequence["items"] = {"anyOf": [sequence["items"], param]}
    else:
        properties[field] = {"anyOf": [properties[field], param]}
    with pytest.raises(AssertionError):
        _assert_definition(
            changed, "A.expr.count-objects", COUNTS["A.expr.count-objects"]
        )


def test_count_drift_detects_removed_filter_exclusions() -> None:
    changed = deepcopy(NODES)
    object_value(object_value(changed["$defs"])["A.expr.count-objects"]).pop("not")
    with pytest.raises(AssertionError):
        _assert_definition(
            changed, "A.expr.count-objects", COUNTS["A.expr.count-objects"]
        )


@pytest.mark.parametrize(
    "name",
    [
        "A.expr.objects-of",
        "A.same-as",
        "A.expr.count-of-filtered",
        "A.cond.did-filtered",
        "A.expr.count-of",
        "A.cond.did",
    ],
)
@pytest.mark.parametrize("mutation", ["required", "discriminator", "exclusive"])
def test_receipt_drift_detects_variant_changes(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    definition = object_value(object_value(changed["$defs"])[name])
    properties = object_value(definition["properties"])
    if mutation == "required":
        definition["required"] = []
    elif mutation == "discriminator":
        key = next(iter(properties))
        properties["wrong_key"] = properties.pop(key)
        required = definition["required"]
        assert isinstance(required, list)
        definition["required"] = [
            "wrong_key" if item == key else item for item in required
        ]
    else:
        properties["follow"] = {"$ref": "#/$defs/A.follow-source"}
    if name in NUMERIC:
        with pytest.raises(AssertionError):
            _assert_numeric_definition(changed, name)
    else:
        with pytest.raises(AssertionError):
            _assert_definition(changed, name, RECEIPTS[name])


@pytest.mark.parametrize("mutation", ["nonempty", "nullable", "source_key"])
def test_material_filter_drift_detects_changes(mutation: str) -> None:
    changed = deepcopy(NODES)
    definition = object_value(object_value(changed["$defs"])["A.material-filter"])
    props = object_value(definition["properties"])
    if mutation == "nonempty":
        definition.pop("minProperties")
    elif mutation == "nullable":
        props["name"] = {"anyOf": [props["name"], {"type": "null"}]}
    else:
        props["in"] = {"$ref": "#/$defs/A.ref"}
    with pytest.raises(AssertionError):
        _assert_definition(changed, "A.material-filter", RECEIPTS["A.material-filter"])


def test_drift_check_does_not_ignore_reference_sibling_constraints() -> None:
    changed = deepcopy(NODES)
    definition = object_value(object_value(changed["$defs"])["A.snapshot-read"])
    definition["minProperties"] = 3
    with pytest.raises(AssertionError):
        _assert_definition(changed, "A.snapshot-read", RECEIPTS["A.snapshot-read"])


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
    assert_selector_definition(schema, name)


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


@pytest.mark.parametrize("mutation", ["missing", "duplicate", "integer"])
def test_numeric_operation_drift_detects_shared_branch_changes(
    mutation: str,
) -> None:
    changed = deepcopy(NODES)
    branches = object_value(object_value(changed["$defs"])["A.expr.numeric-operation"])[
        "anyOf"
    ]
    assert isinstance(branches, list)
    if mutation == "missing":
        branches.pop()
    elif mutation == "duplicate":
        branches.append(deepcopy(branches[0]))
    else:
        branches.append({"$ref": "#/$defs/Int"})
    with pytest.raises(AssertionError):
        _assert_numeric_definition(changed, "A.expr.numeric-operation")


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
            operands[0] = {"$ref": "#/$defs/A.cond"}
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


@pytest.mark.parametrize(
    "name", ["A.collection.in", "A.collection.objects", "A.expr.count-objects"]
)
def test_drift_catches_counted_rights_in_except(name: str) -> None:
    changed = deepcopy(NODES)
    props = object_value(
        object_value(object_value(changed["$defs"])[name])["properties"]
    )
    props["except"] = {"$ref": "#/$defs/A.expr.count-source"}
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize("field", ["who", "zone"])
def test_drift_catches_collection_parameter_rights(field: str) -> None:
    changed = deepcopy(NODES)
    name = "A.collection.objects"
    props = object_value(
        object_value(object_value(changed["$defs"])[name])["properties"]
    )
    props[field] = {"anyOf": [props[field], {"$ref": "#/$defs/A.param"}]}
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "name",
    [
        "A.expr.hand",
        "A.expr.leader-life",
        "A.expr.counter",
        "A.expr.counter-payload",
        "A.expr.agg",
        "A.expr.agg-payload",
        "A.expr.distinct-names",
        "A.expr.distinct-orig-costs",
        "A.cond.exists",
        "A.cond.in-zone",
        "A.cond.in-zone-payload",
        "A.cond.entered-from",
        "A.cond.played-from",
        "A.cond.from-payload",
        "A.cond.entered-by",
        "A.cond.entered-by-payload",
        "A.cond.is",
        "A.cond.is-payload",
        "A.cond.on-field",
        "A.cond.standing",
        "A.cond.acted",
        "A.cond.evolved",
        "A.cond.my-turn",
        "A.cond.opp-turn",
        "A.cond.construction",
        "A.cond.combo",
        "A.cond.spellchain",
        "A.cond.necro",
        "A.cond.awakened",
        "A.cond.crimson",
        "A.cond.hand",
        "A.cond.leader-life",
        "A.cond.zone-free",
        "A.cond.player-eq-threshold",
        "A.cond.player-le-threshold",
        "A.cond.player-ge-threshold",
        "A.cond.zone-free-eq-threshold",
        "A.cond.zone-free-ge-threshold",
        "A.collection.in",
        "A.collection.objects",
        "A.collection.union",
    ],
)
@pytest.mark.parametrize("mutation", ["required", "extra", "tag"])
def test_new_variant_drift_mutations(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    if mutation == "required":
        node["required"] = []
    elif mutation == "extra":
        node["additionalProperties"] = True
    else:
        props = object_value(node["properties"])
        props["wrong_key"] = props.pop(next(iter(props)))
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "name",
    [
        "A.expr.count-atom",
        "A.expr.count-source",
        "A.collection.atom",
        "A.collection",
        "A.cond.player-threshold",
        "A.cond.zone-free-threshold",
        "A.cond",
    ],
)
@pytest.mark.parametrize("mutation", ["missing", "duplicate", "domain"])
def test_query_union_drift_detects_changes(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    branches = object_value(object_value(changed["$defs"])[name])["anyOf"]
    assert isinstance(branches, list)
    if mutation == "missing":
        branches.pop()
    elif mutation == "duplicate":
        branches.append(deepcopy(branches[0]))
    else:
        branches[0] = {"$ref": "#/$defs/A.ref"}
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    "mutation",
    [
        "signed_filter",
        "count_power_max",
        "condition_false",
        "combo_zero",
        "agg_unknown",
        "threshold_param",
    ],
)
def test_query_drift_detects_value_domain_changes(mutation: str) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    changes: dict[str, tuple[str, str, JsonValue]] = {
        "signed_filter": (
            "A.sel.in-objects",
            "cost_le",
            {"$ref": "#/$defs/A.expr.numeric"},
        ),
        "count_power_max": (
            "A.expr.count-objects",
            "power_max",
            {"const": True, "type": "boolean"},
        ),
        "condition_false": ("A.cond.awakened", "awakened", {"type": "boolean"}),
        "combo_zero": ("A.cond.combo", "combo", {"$ref": "#/$defs/Nat"}),
        "agg_unknown": (
            "A.expr.agg-payload",
            "attr",
            {"enum": ["cost", "orig_cost", "power", "hp", "unknown"], "type": "string"},
        ),
        "threshold_param": (
            "A.cond.player-eq-threshold",
            "eq",
            {"$ref": "#/$defs/A.selection-quantity"},
        ),
    }
    name, field, value = changes[mutation]
    object_value(object_value(definitions[name])["properties"])[field] = value
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


NAMED_HISTORY_ROLES = [
    (name, field, leaf, array)
    for name in ("A.event-subject-filter", "A.event-object-sel", "A.material-filter")
    for field, leaf, array in (
        ("name", "CardName", False),
        ("trait", "Concept", False),
        ("not_class", "Concept", False),
        *(
            (
                ("class", "Concept", False),
                ("not_trait", "Concept", False),
                ("keyword", "Concept", False),
                ("traits_any", "Concept", True),
                ("traits_all", "Concept", True),
                ("names", "CardName", True),
            )
            if name != "A.material-filter"
            else ()
        ),
    )
]

NAMED_EFFECT_ROLES = [
    ("A.effect.keywords", "list", "Concept", "items"),
    ("A.effect.also-named", "name", "CardName", "value"),
    ("A.effect.trait", "trait", "Concept", "value"),
    ("A.effect.make-type", "type", "CardKind", "value"),
    *[
        ("A.effect.card-predicate", field, leaf, position)
        for field, leaf, position in (
            ("kind", "CardKind", "union"),
            ("name", "CardName", "union"),
            ("class", "Concept", "value"),
            ("not_class", "Concept", "value"),
            ("trait", "Concept", "value"),
            ("not_trait", "Concept", "value"),
            ("keyword", "Concept", "value"),
            ("traits_any", "Concept", "items"),
            ("traits_all", "Concept", "items"),
            ("names", "CardName", "items"),
        )
    ],
]


def _effect_role_leaf(
    definitions: dict[str, JsonValue], name: str, field: str, position: str
) -> dict[str, JsonValue]:
    node = object_value(
        object_value(object_value(definitions[name])["properties"])[field]
    )
    if position == "items":
        return object_value(node["items"])
    if position == "union":
        branches = node["anyOf"]
        assert isinstance(branches, list)
        return object_value(branches[0])
    assert position == "value"
    return node


def _assert_effect_names_are_referenced(definitions: dict[str, JsonValue]) -> None:
    for name, field, leaf, position in NAMED_EFFECT_ROLES:
        assert _effect_role_leaf(definitions, name, field, position) == {
            "$ref": "#/$defs/" + leaf
        }


def _assert_shared_definitions_are_referenced(schema: dict[str, JsonValue]) -> None:
    definitions = object_value(schema["$defs"])
    protected = (
        "Zone",
        "Code",
        "Nat",
        "Phase",
        "A.receipt-ref",
        "A.entry-payment-ref",
        "A.leader-recipient",
        "BindingName",
        "PlayerSetName",
        "A.same-as",
        "A.expr.objects-of",
    )

    def visit(node: JsonValue, owner: str) -> None:
        if isinstance(node, dict):
            for name in protected:
                assert node != definitions[name] or owner == name
            for child in node.values():
                visit(child, owner)
        elif isinstance(node, list):
            for child in node:
                visit(child, owner)

    for owner, node in definitions.items():
        visit(node, owner)
    for name, field, leaf, array in NAMED_HISTORY_ROLES:
        properties = object_value(object_value(definitions[name])["properties"])
        node = object_value(properties[field])
        if array:
            node = object_value(node["items"])
        assert node == {"$ref": "#/$defs/" + leaf}
    _assert_effect_names_are_referenced(definitions)
    construction = object_value(definitions["A.cond.construction"])
    assert object_value(construction["properties"])["construction"] == {
        "$ref": "#/$defs/Title"
    }


def test_shared_author_atoms_have_one_definition() -> None:
    _assert_shared_definitions_are_referenced(NODES)


@pytest.mark.parametrize(
    ("leaf", "name", "field"),
    [
        ("Zone", "A.cond.in-zone-payload", "zone"),
        ("Code", "A.cond.is-payload", "kind"),
        ("BindingName", "A.collection.in", "as"),
        ("PlayerSetName", "A.collection.objects", "who"),
        ("A.same-as", "A.sel.in-objects", "name"),
        ("A.expr.objects-of", "A.collection.in", "in"),
        ("Title", "A.cond.construction", "construction"),
        ("Nat", "A.event.counter-reached-payload", "value"),
        ("Phase", "A.history.phase-where", "which"),
        ("A.receipt-ref", "A.cond.did-filtered", "result"),
    ],
)
def test_inlining_named_atoms_fails_structural_check(
    leaf: str, name: str, field: str
) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    properties = object_value(object_value(definitions[name])["properties"])
    properties[field] = deepcopy(definitions[leaf])
    with pytest.raises(AssertionError):
        _assert_shared_definitions_are_referenced(changed)


@pytest.mark.parametrize(("name", "field", "leaf", "array"), NAMED_HISTORY_ROLES)
def test_inlining_history_role_names_fails_structural_check(
    name: str, field: str, leaf: str, array: bool
) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    properties = object_value(object_value(definitions[name])["properties"])
    if array:
        object_value(properties[field])["items"] = deepcopy(definitions[leaf])
    else:
        properties[field] = deepcopy(definitions[leaf])
    with pytest.raises(AssertionError):
        _assert_shared_definitions_are_referenced(changed)


@pytest.mark.parametrize(("name", "field", "leaf", "position"), NAMED_EFFECT_ROLES)
def test_inlining_effect_role_names_fails_structural_check(
    name: str, field: str, leaf: str, position: str
) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    node = _effect_role_leaf(definitions, name, field, position)
    node.clear()
    node.update(deepcopy(object_value(definitions[leaf])))
    with pytest.raises(AssertionError):
        _assert_shared_definitions_are_referenced(changed)


@pytest.mark.parametrize(
    ("name", "field", "leaf"),
    [
        ("A.event-recipient-atom", None, "A.leader-recipient"),
        ("A.paid-receipt", "receipt", "A.entry-payment-ref"),
    ],
)
def test_inlining_named_union_branch_fails_structural_check(
    name: str, field: str | None, leaf: str
) -> None:
    changed = deepcopy(NODES)
    definitions = object_value(changed["$defs"])
    node = object_value(definitions[name])
    if field is not None:
        node = object_value(object_value(node["properties"])[field])
    variants = node["anyOf"]
    assert isinstance(variants, list)
    assert variants[1] == {"$ref": "#/$defs/" + leaf}
    variants[1] = deepcopy(definitions[leaf])
    with pytest.raises(AssertionError):
        _assert_shared_definitions_are_referenced(changed)


@pytest.mark.parametrize("name", STEPS)
def test_step_type_drift(name: str) -> None:
    assert_selector_definition(NODES, name)


@pytest.mark.parametrize("name", ABILITIES)
def test_ability_type_drift(name: str) -> None:
    assert_selector_definition(NODES, name)


@pytest.mark.parametrize("name", ENTRIES)
def test_entry_type_drift(name: str) -> None:
    assert_selector_definition(NODES, name)


def _assert_entry_heads_match(schema: dict[str, JsonValue]) -> None:
    definitions = object_value(schema["$defs"])
    heads = object_value(object_value(definitions["A.ability-head"])["properties"])
    wrappers = {
        name
        for name in ENTRIES
        if name.startswith("A.ability.") and name != "A.ability.generated"
    }
    assert wrappers == {"A.ability." + key.replace("_", "-") for key in heads}
    for key, payload in heads.items():
        wrapper = object_value(definitions["A.ability." + key.replace("_", "-")])
        properties = object_value(wrapper["properties"])
        assert properties.keys() - {
            "line",
            "section",
            "sources",
            "imports",
            "ports",
        } == {key}
        assert properties[key] == payload
        required = wrapper["required"]
        assert isinstance(required, list)
        assert set(required) == {"line", "sources", "ports", key}


def test_entry_heads_use_exact_shared_payloads() -> None:
    _assert_entry_heads_match(NODES)


@pytest.mark.parametrize(
    "name", [name for name in ENTRIES if name.startswith("A.ability.")]
)
@pytest.mark.parametrize("mutation", ["required", "extra", "type", "reference"])
def test_entry_drift_detects_wrapper_mutations(name: str, mutation: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    if mutation == "required":
        node["required"] = []
    elif mutation == "extra":
        node["additionalProperties"] = True
    elif mutation == "type":
        object_value(node["properties"])["line"] = {"type": "string"}
    else:
        object_value(node["properties"])["sources"] = {"$ref": "#/$defs/Int"}
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize("key", ["fanfare", "quick", "keywords"])
def test_entry_head_matching_detects_payload_mutation(key: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])["A.ability." + key])
    object_value(node["properties"])[key] = {"$ref": "#/$defs/Int"}
    with pytest.raises(AssertionError):
        _assert_entry_heads_match(changed)
