"""Complete Lastword head and payload checks at the source expansion boundary."""

from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl, ParamSource
from sve_carddb.contracts.four_layer import GlossaryReference
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ExpansionError,
    body_version,
)
from sve_carddb.domains.effects.author_sources import (
    SourceError,
    entry_payload,
    parameter_expected,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from tests.support.author_source_fixtures import SourceCandidate, source_context

from ..translations.test_four_layer_classification import classifier
from ..translations.test_four_layer_damage import EXTRA, RAW, TERM, facts
from .test_author_entries import SCHEMAS, ability, document, shard
from .test_author_expansion import generated, registered
from .test_author_source_expansion import ABILITY_PATH

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_sources import SourceContext


def source_case(raw: str = RAW) -> tuple[SourceCandidate, SourceUse]:
    context = facts(raw)
    engine = classifier((TERM,), EXTRA)
    derived = derive_source(raw, context.source, engine, context=context)
    _, binding = derived.bind(0)
    return SourceCandidate(raw, context.source, engine, context), SourceUse(
        binding_id=binding.id,
        occurrence=binding.occurrence_key(),
        ability_scope="ability_0",
    )


def head(amount: JsonValue = 2) -> dict[str, JsonValue]:
    return {"lastword": {"do": [{"damage": {"to": {"leaders": "all"}, "n": amount}}]}}


def companion(source: SourceContext) -> dict[str, JsonValue]:
    record = registered(head({"param": "n"}))
    record["frame_id"] = source.frame.id
    record["params"] = {
        "n": {"type": "Nat", "source": {"slot": "leaf_1", "component": "value"}}
    }
    record["body_version"] = body_version(record)
    return record


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


@pytest.mark.parametrize("amount", [0, 1, 2, 9007199254740991])
def test_inline_and_companion_exact_source_expansion(
    expander: AuthorExpander, amount: int
) -> None:
    candidate, use = source_case(RAW.replace("2", str(amount)))
    context = source_context(candidate, use)
    record = companion(context)
    inline = ability(head(amount))
    reference = generated(record, {"n": {"type": "Nat", "value": amount}})
    for node in (inline, reference):
        node["sources"] = [use.model_dump(mode="json")]
    results = [
        expander.expand_contexts(
            canonical(document(node)),
            shards,
            {ABILITY_PATH: source_context(candidate, use)},
        )
        for node, shards in ((inline, []), (reference, [canonical(shard(record))]))
    ]
    assert results[0].tree == results[1].tree
    for result in results:
        assert result.sidecar[0].source_proof == "none"
    assert entry_payload(context, "ability_0") == {
        **head(amount),
        "ports": {"imports": {}, "exports": {}},
    }


BAD_HEADS: tuple[dict[str, JsonValue], ...] = (
    {
        "lastword": {
            "do": [
                {
                    "damage": {
                        "to": {"leaders": "all"},
                        "n": 2,
                        "distribute": True,
                        "distribute_at": "resolution",
                    }
                }
            ]
        }
    },
    {"lastword": {"do": [{"damage": {"to": {"leaders": "opponents"}, "n": 2}}]}},
    {
        "lastword": {
            "targets": [{"bind": "picked", "from": {"leaders": "all"}, "n": 1}],
            "do": [{"damage": {"to": "picked", "n": 2}}],
        }
    },
    {"lastword": {"do": [{"damage": {"to": {"leaders": "all"}, "n": 3}}]}},
    head({"count": {"zone": "hand", "who": "self"}}),
    {
        "lastword": {
            "do": [*array(object_value(head()["lastword"])["do"]), {"draw": {"n": 1}}]
        }
    },
    {"fanfare": {"do": array(object_value(head()["lastword"])["do"])}},
    {
        "lastword": {
            "do": [
                {
                    "pay": {
                        "cost": {"pp": 1},
                        "do": array(object_value(head()["lastword"])["do"]),
                        "then": [],
                    }
                }
            ]
        }
    },
    {
        "lastword": {
            "do": [
                {
                    "if": {
                        "cond": {"awakened": True},
                        "then": array(object_value(head()["lastword"])["do"]),
                    }
                }
            ]
        }
    },
)


@pytest.mark.parametrize(
    "body",
    BAD_HEADS,
    ids=[
        "allocation",
        "opponent",
        "target",
        "amount",
        "dynamic",
        "extra-step",
        "fanfare",
        "cost",
        "condition",
    ],
)
def test_expansion_rejects_a_similar_author_payload(
    expander: AuthorExpander, body: dict[str, JsonValue]
) -> None:
    candidate, use = source_case()
    node = ability(deepcopy(body))
    node["sources"] = [use.model_dump(mode="json")]
    with pytest.raises(ExpansionError, match="source_head_payload_mismatch"):
        expander.expand_contexts(
            canonical(document(node)),
            [],
            {ABILITY_PATH: source_context(candidate, use)},
        )


@pytest.mark.parametrize("change", ["concept", "role", "extra-slot", "occurrence"])
def test_fixed_head_and_complete_source_slots(change: str) -> None:
    candidate, use = source_case()
    source = source_context(candidate, use)
    if change == "concept":
        source = replace(
            source,
            binding=source.binding.model_copy(
                update={
                    "values": {
                        **source.binding.values,
                        "leaf_0": GlossaryReference(
                            kind="glossary", key="term:ability.other"
                        ),
                    }
                }
            ),
        )
    elif change == "occurrence":
        source = replace(
            source,
            binding=source.binding.model_copy(
                update={"occurrences": source.binding.occurrences[:1]}
            ),
        )
    else:
        slots = source.frame.leaf_schema.slots
        changed = slots[1].model_copy(
            update={"role": "count"} if change == "role" else {"name": "leaf_2"}
        )
        schema = source.frame.leaf_schema.model_copy(
            update={
                "slots": (slots[0], changed) if change == "role" else (*slots, changed)
            }
        )
        source = replace(
            source, frame=source.frame.model_copy(update={"leaf_schema": schema})
        )
    with pytest.raises(SourceError, match="source_lastword_contract_mismatch"):
        entry_payload(source, "ability_0")


@pytest.mark.parametrize(
    ("slot", "owner", "path", "reason"),
    [
        ("leaf_0", "A.act.damage", ("n",), "parameter_source_role_mismatch"),
        ("leaf_1", "A.act.draw", ("n",), "parameter_source_role_mismatch"),
    ],
)
def test_equal_values_cannot_cross_source_roles(
    slot: str, owner: str, path: tuple[str, ...], reason: str
) -> None:
    candidate, use = source_case()
    source = source_context(candidate, use)
    with pytest.raises(SourceError, match=reason):
        parameter_expected(
            source,
            ParamDecl(type="Nat", source=ParamSource(slot=slot, component="value")),
            owner,
            path,
            {"n": {"param": "n"}},
        )


@pytest.mark.parametrize(
    ("change", "reason"),
    [
        ("payload", "source_head_payload_mismatch"),
        ("slot", "parameter_source_role_mismatch"),
        ("value", "source_param_value_mismatch"),
    ],
)
def test_companion_does_not_bypass_source_checks(
    expander: AuthorExpander, change: str, reason: str
) -> None:
    candidate, use = source_case()
    record = companion(source_context(candidate, use))
    if change == "payload":
        damage = object_value(
            object_value(
                array(object_value(object_value(record["body"])["lastword"])["do"])[0]
            )["damage"]
        )
        damage["to"] = {"leaders": "opponents"}
    elif change == "slot":
        object_value(object_value(object_value(record["params"])["n"])["source"])[
            "slot"
        ] = "leaf_0"
    record["body_version"] = body_version(record)
    node = generated(
        record, {"n": {"type": "Nat", "value": 3 if change == "value" else 2}}
    )
    node["sources"] = [use.model_dump(mode="json")]
    with pytest.raises(
        (ExpansionError, SourceError),
        match=reason,
    ):
        expander.expand_contexts(
            canonical(document(node)),
            [canonical(shard(record))],
            {ABILITY_PATH: source_context(candidate, use)},
        )
