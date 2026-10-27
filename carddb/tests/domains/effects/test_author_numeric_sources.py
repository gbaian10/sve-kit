"""The fixed numeric source admits one complete Fanfare and no parameters."""

from dataclasses import replace
from typing import TYPE_CHECKING, Any

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
from sve_carddb.domains.effects.author_inputs import AuthorInputError
from sve_carddb.domains.effects.author_sources import (
    SourceError,
    entry_payload,
    parameter_expected,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_fixed_actions import NUMERIC
from tests.support.author_source_fixtures import SourceCandidate, source_context

from ..translations.test_four_layer_classification import classifier
from ..translations.test_four_layer_fixed_actions import EXTRA, TERMS, facts
from ..translations.test_four_layer_fixed_actions import NUMERIC_RAW as RAW
from .test_author_entries import ability, document, shard
from .test_author_expansion import generated, registered
from .test_author_source_expansion import ABILITY_PATH

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_sources import SourceContext


def source_case() -> tuple[SourceCandidate, SourceUse]:
    context = facts(RAW, NUMERIC)
    engine = classifier(TERMS, EXTRA)
    derived = derive_source(RAW, context.source, engine, context=context)
    _, binding = derived.bind(0)
    return SourceCandidate(RAW, context.source, engine, context), SourceUse(
        binding_id=binding.id,
        occurrence=binding.occurrence_key(),
        ability_scope="ability_0",
    )


def head() -> dict[str, JsonValue]:
    return {
        "fanfare": {
            "do": [
                {
                    "buff": {
                        "on": {
                            "who": "self",
                            "zone": "field",
                            "kind": "follower",
                            "other": True,
                        },
                        "power": 1,
                        "hp": 1,
                    }
                }
            ]
        }
    }


def companion(source: SourceContext) -> dict[str, JsonValue]:
    record = registered(head())
    record["frame_id"] = source.frame.id
    record["body_version"] = body_version(record)
    return record


def test_inline_and_companion_exact_source_expansion(expander: AuthorExpander) -> None:
    candidate, use = source_case()
    source = source_context(candidate, use)
    record = companion(source)
    inline, reference = ability(head()), generated(record, {})
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
    assert all(r.sidecar[0].source_proof == "none" for r in results)
    assert entry_payload(source, "ability_0") == {
        **head(),
        "ports": {"imports": {}, "exports": {}},
    }


def changed_head(change: str) -> dict[str, JsonValue]:
    body = head()
    fanfare = object_value(body["fanfare"])
    steps = array(fanfare["do"])
    buff = object_value(object_value(steps[0])["buff"])
    recipients = object_value(buff["on"])
    if change in {"spell", "lastword", "attack_time"}:
        return {change: fanfare}
    if change in {"opponent", "each"}:
        recipients["who"] = "opp" if change == "opponent" else "each"
    elif change == "include-self":
        del recipients["other"]
    elif change == "hand":
        recipients["zone"] = "hand"
    elif change == "amulet":
        recipients["kind"] = "amulet"
    elif change == "target":
        fanfare["targets"] = [{"bind": "picked", "from": recipients, "n": 1}]
        buff["on"] = "picked"
    elif change == "dynamic":
        buff["power"] = {"count": {"who": "self", "zone": "hand"}}
    elif change in {"power", "hp"}:
        buff[change] = 2
    elif change == "set":
        object_value(steps[0])["set_stat"] = object_value(steps[0]).pop("buff")
    else:
        change_steps(change, fanfare, steps, buff)
    return body


def change_steps(
    change: str,
    fanfare: dict[str, JsonValue],
    steps: list[JsonValue],
    buff: dict[str, JsonValue],
) -> None:
    if change == "extra-step":
        steps.append({"draw": {"n": 1}})
    elif change == "condition":
        fanfare["do"] = [{"if": {"cond": {"awakened": True}, "then": steps}}]
    elif change == "cost":
        fanfare["do"] = [{"pay": {"cost": {"pp": 1}, "do": steps, "then": []}}]
    elif change == "until":
        buff["until"] = "turn"
    else:
        raise AssertionError(change)


CHANGES = (
    "spell",
    "lastword",
    "attack_time",
    "opponent",
    "each",
    "include-self",
    "hand",
    "amulet",
    "target",
    "dynamic",
    "power",
    "hp",
    "set",
    "extra-step",
    "condition",
    "cost",
    "until",
)


@pytest.mark.parametrize("change", CHANGES)
@pytest.mark.parametrize("macro", [False, True], ids=["inline", "companion"])
def test_complete_payload_cannot_change(
    expander: AuthorExpander, change: str, macro: bool
) -> None:
    candidate, use = source_case()
    body = changed_head(change)
    # Establish structural validity before testing the stricter source boundary.
    expander.schemas.document(canonical(document(ability(body))))
    shards = []
    if macro:
        record = companion(source_context(candidate, use))
        record["body"] = body
        record["body_version"] = body_version(record)
        node = generated(record, {})
        shards = [canonical(shard(record))]
    else:
        node = ability(body)
    node["sources"] = [use.model_dump(mode="json")]
    with pytest.raises(ExpansionError, match="source_head_payload_mismatch"):
        expander.expand_contexts(
            canonical(document(node)),
            shards,
            {ABILITY_PATH: source_context(candidate, use)},
        )


@pytest.mark.parametrize(
    "change",
    [
        "concept",
        "value",
        "bool",
        "type",
        "role",
        "domain",
        "required",
        "occurrence",
        "extra-slot",
        "frame",
        "canonical",
    ],
)
def test_all_fixed_source_operands_are_checked(change: str) -> None:
    candidate, use = source_case()
    source = source_context(candidate, use)
    if change in {"concept", "value", "bool"}:
        values = dict(source.binding.values)
        values["leaf_0" if change == "concept" else "leaf_2"] = (
            GlossaryReference(kind="glossary", key="term:ability.last_words")
            if change == "concept"
            else True
            if change == "bool"
            else 2
        )
        source = replace(
            source, binding=source.binding.model_copy(update={"values": values})
        )
    elif change == "occurrence":
        source = replace(
            source,
            binding=source.binding.model_copy(
                update={"occurrences": source.binding.occurrences[:-1]}
            ),
        )
    elif change == "frame":
        source = replace(
            source,
            binding=source.binding.model_copy(update={"frame_id": "frame:wrong"}),
        )
    elif change == "canonical":
        source = replace(
            source, canonical_source=source.canonical_source.replace("他の", "")
        )
    else:
        slots = source.frame.leaf_schema.slots
        changes: dict[str, dict[str, Any]] = {
            "type": {"type": "Int"},
            "role": {"role": "count"},
            "domain": {"domain": slots[2].domain.model_copy(update={"max": 1})},
            "required": {"required": False},
            "extra-slot": {"name": "leaf_5"},
        }
        changed = slots[2].model_copy(update=changes[change])
        schema = source.frame.leaf_schema.model_copy(
            update={
                "slots": (*slots, changed)
                if change == "extra-slot"
                else (*slots[:2], changed, *slots[3:])
            }
        )
        source = replace(
            source, frame=source.frame.model_copy(update={"leaf_schema": schema})
        )
    with pytest.raises(SourceError, match="source_numeric_contract_mismatch"):
        entry_payload(source, "ability_0")


@pytest.mark.parametrize("slot", ["leaf_0", "leaf_1", "leaf_2", "leaf_3", "leaf_4"])
@pytest.mark.parametrize(
    ("owner", "path"),
    [("A.act.buff", ("power",)), ("A.act.buff", ("hp",)), ("A.act.draw", ("n",))],
)
def test_numeric_constants_are_not_parameters(
    slot: str, owner: str, path: tuple[str, ...]
) -> None:
    candidate, use = source_case()
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        parameter_expected(
            source_context(candidate, use),
            ParamDecl(type="Int", source=ParamSource(slot=slot, component="value")),
            owner,
            path,
            {path[0]: {"param": "n"}},
        )


@pytest.mark.parametrize("stat", ["power", "hp"])
def test_companion_cannot_parameterize_stat_constants(
    expander: AuthorExpander, stat: str
) -> None:
    candidate, use = source_case()
    record = companion(source_context(candidate, use))
    buff = object_value(
        object_value(
            array(object_value(object_value(record["body"])["fanfare"])["do"])[0]
        )["buff"]
    )
    buff[stat] = {"param": "delta"}
    record["params"] = {
        "delta": {
            "type": "Int",
            "source": {
                "slot": "leaf_2" if stat == "power" else "leaf_4",
                "component": "value",
            },
        }
    }
    record["body_version"] = body_version(record)
    node = generated(record, {"delta": {"type": "Int", "value": 1}})
    node["sources"] = [use.model_dump(mode="json")]
    # The buff stat fields take numeric expressions only, so the schema rejects
    # a ParamRef before any parameter-site check can run.
    with pytest.raises(AuthorInputError, match="schema"):
        expander.expand_contexts(
            canonical(document(node)),
            [canonical(shard(record))],
            {ABILITY_PATH: source_context(candidate, use)},
        )


@pytest.mark.parametrize("slot", ["leaf_1", "leaf_2", "leaf_3", "leaf_4"])
def test_each_stat_operand_retains_its_own_meaning(slot: str) -> None:
    candidate, use = source_case()
    source = source_context(candidate, use)
    values = dict(source.binding.values)
    values[slot] = (
        2
        if slot in {"leaf_2", "leaf_4"}
        else GlossaryReference(kind="glossary", key="term:stat.other")
    )
    source = replace(
        source, binding=source.binding.model_copy(update={"values": values})
    )
    with pytest.raises(SourceError, match="source_numeric_contract_mismatch"):
        entry_payload(source, "ability_0")


def test_companion_cannot_borrow_a_numeric_leaf_for_draw(
    expander: AuthorExpander,
) -> None:
    candidate, use = source_case()
    record = companion(source_context(candidate, use))
    record["body"] = {
        "fanfare": {"do": [{"draw": {"who": "self", "n": {"param": "n"}}}]}
    }
    record["params"] = {
        "n": {"type": "Nat", "source": {"slot": "leaf_2", "component": "value"}}
    }
    record["body_version"] = body_version(record)
    node = generated(record, {"n": {"type": "Nat", "value": 1}})
    node["sources"] = [use.model_dump(mode="json")]
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        expander.expand_contexts(
            canonical(document(node)),
            [canonical(shard(record))],
            {ABILITY_PATH: source_context(candidate, use)},
        )
