"""Named searches retain exact quantity, fixed references and the complete Lastword."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.author_leaves import ParamDecl
from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Constant,
    GlossaryReference,
    QuantitySpec,
)
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ExpansionError,
    ability_nodes,
    body_version,
    equivalent_ability,
)
from sve_carddb.domains.effects.author_sources import (
    SourceError,
    entry_payload,
    parameter_expected,
)
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from tests.support.author_source_fixtures import source_context

from ..translations.test_four_layer_classification import classifier
from ..translations.test_four_layer_fixed_actions import EXTRA, SEARCH_RAW, TERMS
from .author_search_cases import PATH, head, search_ability, search_source
from .test_author_entries import SCHEMAS, ability, document, shard
from .test_author_expansion import generated, registered

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_sources import SourceContext
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


@pytest.mark.parametrize("count", [0, 1, 2, 9007199254740991])
def test_search_inline_and_companion_are_source_equivalent(
    expander: AuthorExpander, count: int
) -> None:
    candidate, use, frame = search_source(SEARCH_RAW.replace("1枚", f"{count}枚"))
    results = []
    for companion in (False, True):
        entry, bodies = search_ability(frame, use, companion=companion, count=count)
        result = expander.expand_contexts(
            canonical(document(entry)), bodies, {PATH: source_context(candidate, use)}
        )
        assert result.sidecar[0].source_proof == "none"
        assert result.sidecar[0].frame_id == frame.id
        assert equivalent_ability(next(ability_nodes(result.tree))[1]) == canonical(
            head(count) | {"ports": {"imports": {}, "exports": {}}}
        )
        results.append(result.tree)
    assert results[0] == results[1]


def bad_payload(change: str) -> dict[str, JsonValue]:
    payload = head()
    body = object_value(payload["lastword"])
    search = object_value(object_value(array(body["do"])[0])["search"])
    selector = object_value(search["sel"])
    if change in {"fanfare", "attack_time", "spell"}:
        return {change: body}
    if change in {"opponent", "ex-source", "name", "filter"}:
        selector.update(
            {
                "opponent": {"who": "opp"},
                "ex-source": {"zone": "ex"},
                "name": {"name": "聖杖の司祭"},
                "filter": {"kind": "follower"},
            }[change]
        )
    elif change == "destination":
        search["to"] = "ex"
    elif change == "up-to":
        search["upto"] = search.pop("n")
    elif change == "count":
        search["n"] = 2
    elif change == "dynamic":
        body["define"] = {"X": 1}
        search["n"] = "X"
    elif change == "condition":
        search["precondition"] = {"cmp": {"left": 1, "op": "eq", "right": 1}}
    elif change == "cost":
        body["optional_cost"] = {"pp": 1}
    elif change == "extra-action":
        array(body["do"]).append({"draw": {"n": 1}})
    else:
        body["do"] = []
    return payload


@pytest.mark.parametrize(
    "change",
    [
        "fanfare",
        "attack_time",
        "spell",
        "opponent",
        "ex-source",
        "name",
        "filter",
        "destination",
        "up-to",
        "count",
        "dynamic",
        "condition",
        "cost",
        "extra-action",
        "empty",
    ],
)
@pytest.mark.parametrize("companion", [False, True])
def test_search_rejects_complete_payload_drift(
    expander: AuthorExpander, change: str, *, companion: bool
) -> None:
    candidate, use, frame = search_source()
    bad = bad_payload(change)
    if companion:
        record = registered(bad)
        record["frame_id"] = frame.id
        entry, bodies = generated(record), [canonical(shard(record))]
    else:
        entry, bodies = ability(bad), []
    entry["sources"] = [use.model_dump(mode="json")]
    raw = canonical(document(entry))
    expander.expand(raw, bodies)
    with pytest.raises(ExpansionError, match="source_head_payload_mismatch"):
        expander.expand_contexts(raw, bodies, {PATH: source_context(candidate, use)})


@pytest.mark.parametrize(
    ("change", "reason"),
    [
        ("value", "source_param_value_mismatch"),
        ("component", "param_component_mismatch"),
        ("slot", "parameter_source_role_mismatch"),
        ("name-param", "parameter_source_role_mismatch"),
        ("zone-param", "parameter_source_operand_mismatch"),
    ],
)
def test_search_parameters_only_bind_the_exact_search_count(
    expander: AuthorExpander, change: str, reason: str
) -> None:
    candidate, use, frame = search_source()
    record = registered(head({"param": "n"}))
    record["frame_id"] = frame.id
    declaration: dict[str, JsonValue] = {
        "type": "Nat",
        "source": {"slot": "leaf_4", "component": "constant"},
    }
    value: JsonValue = 2 if change == "value" else 1
    if change == "component":
        object_value(declaration["source"])["component"] = "value"
    elif change == "slot":
        object_value(declaration["source"])["slot"] = "leaf_0"
    elif change in {"name-param", "zone-param"}:
        record["body"] = head()
        search = object_value(
            object_value(
                array(object_value(object_value(record["body"])["lastword"])["do"])[0]
            )["search"]
        )
        if change == "name-param":
            object_value(search["sel"])["name"] = {"param": "n"}
            declaration = {
                "type": "CardName",
                "source": {"slot": "leaf_3", "component": "value"},
            }
            value = "式神の使役"
        else:
            search["to"] = {"param": "n"}
            declaration = {
                "type": "Zone",
                "source": {"slot": "leaf_5", "component": "value"},
            }
            value = "hand"
    record["params"] = {"n": declaration}
    record["body_version"] = body_version(record)
    entry = generated(record, {"n": {"type": declaration["type"], "value": value}})
    entry["sources"] = [use.model_dump(mode="json")]
    with pytest.raises((SourceError, ExpansionError), match=reason):
        expander.expand_contexts(
            canonical(document(entry)),
            [canonical(shard(record))],
            {PATH: source_context(candidate, use)},
        )


def context() -> SourceContext:
    candidate, use, _ = search_source()
    return source_context(candidate, use)


@pytest.mark.parametrize(
    "change",
    [
        "head",
        "owner",
        "zone",
        "name",
        "destination",
        "implicit-owner",
        "schema-role",
        "schema-domain",
        "optional",
        "occurrence",
        "frame",
        "line",
        "field",
        "canonical",
    ],
)
def test_search_checks_all_fixed_source_leaves(change: str) -> None:
    source = context()
    binding = source.binding
    values = dict(binding.values)
    fixed = {
        "head": (
            "leaf_0",
            GlossaryReference(kind="glossary", key="term:ability.fanfare"),
        ),
        "owner": ("leaf_1", "opponent"),
        "zone": ("leaf_2", ("ex",)),
        "name": (
            "leaf_3",
            CardNameReference(kind="card_name", term_id="term:name.other"),
        ),
        "destination": ("leaf_5", ("ex",)),
        "implicit-owner": ("leaf_6", "opponent"),
    }
    if change in fixed:
        slot_name, value = fixed[change]
        binding = binding.model_copy(update={"values": values | {slot_name: value}})
    elif change in {"schema-role", "schema-domain", "optional"}:
        slots = list(source.frame.leaf_schema.slots)
        slot = slots[4]
        slots[4] = slot.model_copy(
            update={"role": "count"}
            if change == "schema-role"
            else {
                "domain": slot.domain.model_copy(
                    update={"values": ("quantity.selection_count.bound.v1",)}
                )
            }
            if change == "schema-domain"
            else {"required": False}
        )
        source = replace(
            source,
            frame=source.frame.model_copy(
                update={
                    "leaf_schema": source.frame.leaf_schema.model_copy(
                        update={"slots": tuple(slots)}
                    )
                }
            ),
        )
    elif change == "occurrence":
        binding = binding.model_copy(update={"occurrences": binding.occurrences[:-1]})
    elif change == "frame":
        binding = binding.model_copy(update={"frame_id": "frame:" + "0" * 64})
    elif change == "line":
        binding = binding.model_copy(update={"line_ordinal": 1})
    elif change == "field":
        binding = binding.model_copy(
            update={
                "source": binding.source.model_copy(
                    update={"field": "section", "ordinal": 0}
                )
            }
        )
    else:
        source = replace(source, canonical_source=source.canonical_source + "extra")
    source = replace(source, binding=binding)
    with pytest.raises(SourceError, match="source_search_contract_mismatch"):
        entry_payload(source, "ability_0")


@pytest.mark.parametrize(
    "value",
    [
        True,
        1,
        QuantitySpec(mode="up_to", expr=Constant(kind="constant", value=1)),
        QuantitySpec(
            mode="exact",
            expr=Constant(kind="constant", value=1).model_copy(update={"value": True}),
        ),
    ],
    ids=["bool", "scalar", "up-to", "bool-constant"],
)
def test_search_quantity_is_an_exact_safe_constant(value: object) -> None:
    source = context()
    source = replace(
        source,
        binding=source.binding.model_copy(
            update={"values": source.binding.values | {"leaf_4": value}}
        ),
    )
    with pytest.raises(SourceError, match="source_search_operand_mismatch"):
        entry_payload(source, "ability_0")


def test_search_fixed_name_requires_exact_japanese_catalog_term() -> None:
    source = context()
    terms = tuple(
        Term(term.id, term.category, "別の名前")
        if term.id == "term:name.shikigami_employment"
        else term
        for term in TERMS
    )
    source = replace(source, classifier=classifier(terms, EXTRA))
    with pytest.raises(SourceError, match="source_search_contract_mismatch"):
        entry_payload(source, "ability_0")


@pytest.mark.parametrize(
    ("owner", "path", "reason"),
    [
        ("A.act.choose.exact", ("n",), "parameter_source_operand_mismatch"),
        ("A.act.search.up-to", ("upto",), "parameter_source_mode_mismatch"),
    ],
)
def test_search_count_cannot_be_borrowed_by_other_selection(
    owner: str, path: tuple[str, ...], reason: str
) -> None:
    declaration = ParamDecl.model_validate(
        {"type": "Nat", "source": {"slot": "leaf_4", "component": "constant"}}
    )
    with pytest.raises(SourceError, match=reason):
        parameter_expected(
            context(), declaration, owner, path, {path[0]: {"param": "n"}}
        )


@pytest.mark.parametrize(
    "change", ["historical", "evolved", "spell", "section", "token", "no-context"]
)
def test_search_requires_complete_normal_current_follower_context(change: str) -> None:
    candidate, _, _ = search_source()
    assert candidate.context is not None
    contexts: dict[str, CardContext | None] = {
        "historical": replace(candidate.context, current=False),
        "evolved": replace(candidate.context, phase="evolved"),
        "spell": replace(candidate.context, card_kind="spell"),
        "section": replace(candidate.context, section_count=1),
        "token": replace(candidate.context, special_kinds=frozenset({"token"})),
        "no-context": None,
    }
    derived = derive_source(
        candidate.raw,
        candidate.descriptor,
        candidate.classifier,
        context=contexts[change],
    )
    frame, _ = derived.bind(0)
    assert frame.semantic_variant.state == "pending"
