"""Complete Advance Fanfare sources and fixed, parameter-free author payloads."""

from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.four_layer import GlossaryReference
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ExpansionError,
    body_version,
)
from sve_carddb.domains.effects.author_sources import (
    SourceCandidate,
    SourceError,
    entry_payload,
    resolve_source,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source

from ..translations.test_four_layer_classification import classifier
from ..translations.test_four_layer_destroy import EXTRA, RAW, TERMS, facts
from .test_author_entries import SCHEMAS, ability, document, shard
from .test_author_expansion import generated, registered
from .test_author_source_expansion import ABILITY_PATH

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_sources import SourceContext


def source_case() -> tuple[SourceCandidate, SourceUse]:
    context = facts()
    engine = classifier(TERMS, EXTRA)
    derived = derive_source(RAW, context.source, engine, context=context)
    _, binding = derived.bind(2)
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
                    "destroy": {
                        "ref": {"who": "opp", "zone": "field", "kind": "follower"}
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


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


def test_inline_and_companion_preserve_complete_fanfare(
    expander: AuthorExpander,
) -> None:
    candidate, use = source_case()
    context = resolve_source(candidate, use)
    record = companion(context)
    inline = ability(head())
    reference = generated(record)
    results = []
    for node, shards in ((inline, []), (reference, [canonical(shard(record))])):
        node["line"] = 2
        node["sources"] = [use.model_dump(mode="json")]
        results.append(
            expander.expand_sources(
                canonical(document(node)), shards, {ABILITY_PATH: candidate}
            )
        )
    assert results[0].tree == results[1].tree
    assert all(r.sidecar[0].source_proof == "derived_self_consistent" for r in results)
    assert entry_payload(context, "ability_0") == {
        **head(),
        "ports": {"imports": {}, "exports": {}},
    }


BAD_HEADS: tuple[dict[str, JsonValue], ...] = (
    {
        "lastword": {
            "do": [
                {
                    "destroy": {
                        "ref": {"who": "opp", "zone": "field", "kind": "follower"}
                    }
                }
            ]
        }
    },
    {
        "attack_time": {
            "do": [
                {
                    "destroy": {
                        "ref": {"who": "opp", "zone": "field", "kind": "follower"}
                    }
                }
            ]
        }
    },
    {
        "fanfare": {
            "do": [
                {
                    "destroy": {
                        "ref": {"who": "self", "zone": "field", "kind": "follower"}
                    }
                }
            ]
        }
    },
    {
        "fanfare": {
            "do": [
                {
                    "destroy": {
                        "ref": {"who": "turn", "zone": "field", "kind": "follower"}
                    }
                }
            ]
        }
    },
    {
        "fanfare": {
            "do": [
                {"destroy": {"ref": {"who": "opp", "zone": "ex", "kind": "follower"}}}
            ]
        }
    },
    {
        "fanfare": {
            "do": [
                {"destroy": {"ref": {"who": "opp", "zone": "field", "kind": "amulet"}}}
            ]
        }
    },
    {
        "fanfare": {
            "targets": [
                {
                    "bind": "picked",
                    "from": {"who": "opp", "zone": "field", "kind": "follower"},
                    "n": 1,
                }
            ],
            "do": [{"destroy": {"ref": "picked"}}],
        }
    },
    {
        "fanfare": {
            "do": [
                {
                    "if": {
                        "cond": {"awakened": True},
                        "then": [
                            {
                                "destroy": {
                                    "ref": {
                                        "who": "opp",
                                        "zone": "field",
                                        "kind": "follower",
                                    }
                                }
                            }
                        ],
                    }
                }
            ]
        }
    },
    {
        "fanfare": {
            "do": [
                {
                    "pay": {
                        "cost": {"pp": 1},
                        "do": [
                            {
                                "destroy": {
                                    "ref": {
                                        "who": "opp",
                                        "zone": "field",
                                        "kind": "follower",
                                    }
                                }
                            }
                        ],
                        "then": [],
                    }
                }
            ]
        }
    },
    {
        "fanfare": {
            "do": [
                {
                    "destroy": {
                        "ref": {"who": "opp", "zone": "field", "kind": "follower"}
                    }
                },
                {"draw": {"n": 1}},
            ]
        }
    },
)


@pytest.mark.parametrize(
    "body",
    BAD_HEADS,
    ids=[
        "lastword",
        "attack",
        "self",
        "turn",
        "ex",
        "amulet",
        "target-one",
        "condition",
        "cost",
        "extra-step",
    ],
)
@pytest.mark.parametrize("reference", [False, True], ids=["inline", "companion"])
def test_similar_payload_is_not_the_source(
    expander: AuthorExpander, body: dict[str, JsonValue], reference: bool
) -> None:
    candidate, use = source_case()
    record = companion(resolve_source(candidate, use))
    record["body"] = deepcopy(body)
    record["body_version"] = body_version(record)
    node = generated(record) if reference else ability(deepcopy(body))
    node["line"] = 2
    node["sources"] = [use.model_dump(mode="json")]
    with pytest.raises(ExpansionError, match="source_head_payload_mismatch"):
        expander.expand_sources(
            canonical(document(node)),
            [canonical(shard(record))] if reference else [],
            {ABILITY_PATH: candidate},
        )


@pytest.mark.parametrize(
    "change",
    [
        "concept",
        "role",
        "domain",
        "optional",
        "schema-span",
        "occurrence",
        "extra-value",
        "frame",
        "line",
        "field",
        "ordinal",
        "segments",
        "canonical",
    ],
)
def test_complete_fixed_leaf_and_location_contract(change: str) -> None:
    candidate, use = source_case()
    source = resolve_source(candidate, use)
    binding = source.binding
    if change == "concept":
        binding = binding.model_copy(
            update={
                "values": {
                    "leaf_0": GlossaryReference(
                        kind="glossary", key="term:ability.other"
                    )
                }
            }
        )
    elif change == "extra-value":
        binding = binding.model_copy(update={"values": {**binding.values, "leaf_1": 1}})
    elif change == "occurrence":
        binding = binding.model_copy(update={"occurrences": ()})
    elif change == "frame":
        binding = binding.model_copy(update={"frame_id": "frame:" + "0" * 64})
    elif change in {"role", "domain", "optional", "schema-span"}:
        slot = source.frame.leaf_schema.slots[0]
        choices: dict[str, dict[str, object]] = {
            "role": {"role": "count"},
            "domain": {"domain": slot.domain.model_copy(update={"values": ()})},
            "optional": {"required": False},
            "schema-span": {"occurrences": ()},
        }
        updates = choices[change]
        source = replace(
            source,
            frame=source.frame.model_copy(
                update={
                    "leaf_schema": source.frame.leaf_schema.model_copy(
                        update={"slots": (slot.model_copy(update=updates),)}
                    )
                }
            ),
        )
    elif change == "canonical":
        source = replace(source, canonical_source="other")
    elif change in {"field", "ordinal"}:
        binding = binding.model_copy(
            update={
                "source": binding.source.model_copy(
                    update={"field": "section"} if change == "field" else {"ordinal": 0}
                )
            }
        )
    else:
        binding = binding.model_copy(
            update={
                **(
                    {"line_ordinal": 0}
                    if change == "line"
                    else {
                        "source_span": binding.source_span.model_copy(
                            update={"segments": ()}
                        )
                    }
                )
            }
        )
    source = replace(source, binding=binding)
    with pytest.raises(SourceError, match="source_destroy_contract_mismatch"):
        entry_payload(source, "ability_0")


@pytest.mark.parametrize(
    "raw",
    [
        RAW.replace("ファンファーレ", "ラストワード"),
        RAW.replace("ファンファーレ", "攻撃時"),
        RAW.replace("相手", "自分"),
        RAW.replace("相手の場", "場"),
        RAW.replace("すべてを破壊する。", "1体を選ぶ。それを破壊する。"),
        RAW.replace("すべてを破壊する。", "1体まで選ぶ。それを破壊する。"),
        RAW.replace("相手", "【覚醒】状態なら、相手"),
        RAW.replace("相手", "{コスト1}：相手"),
        RAW.replace("【オーラ】", ""),
        RAW.replace("【守護】", ""),
        RAW + "\n1枚引く。",
    ],
)
def test_changed_complete_source_is_rejected(raw: str) -> None:
    candidate, use = source_case()
    candidate = replace(
        candidate, raw=raw, context=facts(raw), descriptor=facts(raw).source
    )
    with pytest.raises(SourceError):
        resolve_source(candidate, use)


@pytest.mark.parametrize(
    "change", ["normal", "evolved", "missing", "sections", "history"]
)
def test_other_holder_context_is_rejected(change: str) -> None:
    candidate, use = source_case()
    context = facts()
    if change in {"normal", "evolved"}:
        context = replace(
            context,
            phase=change,
            special_kinds=frozenset() if change == "normal" else frozenset({"evolve"}),
        )
    elif change == "sections":
        context = replace(context, section_count=1)
    elif change == "history":
        context = replace(context, current=False)
    with pytest.raises(SourceError, match="source_pending"):
        resolve_source(
            replace(candidate, context=None if change == "missing" else context), use
        )


@pytest.mark.parametrize("name", ["n", "who", "zone", "kind", "head"])
def test_companion_has_no_dynamic_parameters(
    expander: AuthorExpander, name: str
) -> None:
    candidate, use = source_case()
    record = companion(resolve_source(candidate, use))
    node = generated(record, {name: {"type": "Nat", "value": 1}})
    node["line"] = 2
    node["sources"] = [use.model_dump(mode="json")]
    with pytest.raises(ExpansionError, match="param_names_mismatch"):
        expander.expand_sources(
            canonical(document(node)),
            [canonical(shard(record))],
            {ABILITY_PATH: candidate},
        )
