"""Draws retain their complete source head, actor, operand and control graph."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ExpansionError,
    ability_nodes,
    body_version,
    equivalent_ability,
)
from sve_carddb.domains.effects.author_sources import (
    SourceCandidate,
    SourceError,
    entry_payload,
    parameter_expected,
    resolve_source,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from ..translations.test_four_layer_classification import classifier, source
from .author_draw_cases import PATH, draw_ability, draw_source, head
from .test_author_entries import SCHEMAS, ability, document, shard
from .test_author_expansion import generated, registered

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import LeafSlot
    from sve_carddb.domains.effects.author_sources import SourceContext


@pytest.fixture(params=["spell", "attack_time"])
def draw_head(request: pytest.FixtureRequest) -> str:
    assert isinstance(request.param, str)
    return request.param


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


@pytest.mark.parametrize("count", [0, 1, 3, 9007199254740991])
@pytest.mark.parametrize("companion", [False, True])
def test_draw_source_expansion(
    expander: AuthorExpander, count: int, draw_head: str, *, companion: bool
) -> None:
    candidate, use, frame = draw_source(f"{count}枚引く。", name=draw_head)
    entry, bodies = draw_ability(
        frame, use, companion=companion, count=count, name=draw_head
    )
    result = expander.expand_sources(
        canonical(document(entry)), bodies, {PATH: candidate}
    )
    expected = canonical(
        head(count, name=draw_head) | {"ports": {"imports": {}, "exports": {}}}
    )
    assert equivalent_ability(next(ability_nodes(result.tree))[1]) == expected
    assert result.sidecar[0].source_proof == "derived_self_consistent"
    assert result.sidecar[0].frame_id == frame.id


@pytest.mark.parametrize(
    ("source_head", "other_head"),
    [
        ("spell", "attack_time"),
        ("spell", "evolve_time"),
        ("attack_time", "spell"),
        ("attack_time", "evolve_time"),
    ],
)
@pytest.mark.parametrize("companion", [False, True])
def test_draw_heads_are_not_interchangeable(
    expander: AuthorExpander, source_head: str, other_head: str, *, companion: bool
) -> None:
    candidate, use, frame = draw_source(name=source_head)
    entry, bodies = draw_ability(frame, use, companion=companion, name=other_head)
    raw = canonical(document(entry))
    expander.expand(raw, bodies)
    with pytest.raises(ExpansionError, match="source_head_payload_mismatch"):
        expander.expand_sources(raw, bodies, {PATH: candidate})


BAD_HEADS: list[dict[str, JsonValue]] = [
    {"fanfare": {"do": [{"draw": {"who": "self", "n": 3}}]}},
    {"evolve_time": {"do": [{"draw": {"who": "self", "n": 3}}]}},
    {"spell": {"do": [{"draw": {"who": "opp", "n": 3}}]}},
    {"spell": {"do": [{"draw": {"who": "each", "n": 3}}]}},
    {"spell": {"do": [{"draw": {"n": 3}}]}},
    head(2),
    {"spell": {"do": []}},
    {"spell": {"do": [{"draw": {"who": "self", "n": 3}}, {"draw": {"n": 1}}]}},
    {"spell": {"define": {"X": 3}, "do": [{"draw": {"who": "self", "n": "X"}}]}},
    {
        "spell": {
            "do": [
                {
                    "draw": {
                        "who": "self",
                        "n": 3,
                        "precondition": {"cmp": {"left": 1, "op": "eq", "right": 1}},
                    }
                }
            ]
        }
    },
    {
        "spell": {
            "do": [{"draw": {"who": "self", "n": 3}}],
            "optional_cost": {"pp": 1},
        }
    },
]


@pytest.mark.parametrize(
    "bad",
    BAD_HEADS,
    ids=[
        "head",
        "evolve-head",
        "opponent",
        "both",
        "implicit-actor",
        "count",
        "empty",
        "extra-action",
        "dynamic",
        "condition",
        "cost",
    ],
)
@pytest.mark.parametrize("companion", [False, True])
def test_draw_rejects_complete_payload_drift(
    expander: AuthorExpander,
    bad: dict[str, JsonValue],
    draw_head: str,
    *,
    companion: bool,
) -> None:
    candidate, use, frame = draw_source(name=draw_head)
    if "spell" in bad:
        bad = {draw_head: bad["spell"]}
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
        expander.expand_sources(raw, bodies, {PATH: candidate})


@pytest.mark.parametrize(
    "raw",
    [
        "3枚まで引く。",
        "相手は3枚引く。",
        "お互いに3枚引く。",
        "自分の場のフォロワーの数と同じ枚数を引く。",
        "自分の手札が3枚以下なら、3枚引く。",
        "手札1枚を捨てる：3枚引く。",
        "{クイック}\n3枚引く。",
        "3枚引いてよい。",
        "03枚引く。",
        "9007199254740992枚引く。",
        "『ナイト』トークン\n3枚引く。",
    ],
    ids=[
        "up-to",
        "opponent",
        "both",
        "dynamic",
        "condition",
        "cost",
        "quick",
        "optional",
        "leading-zero",
        "unsafe",
        "token",
    ],
)
def test_unadmitted_source_constructions_fail_closed(
    expander: AuthorExpander, raw: str, draw_head: str
) -> None:
    if draw_head == "attack_time":
        raw = "【攻撃時】" + raw
    descriptor = source(raw)
    engine = classifier()
    context = CardContext(
        descriptor,
        "c:synthetic",
        "f:synthetic",
        "normal",
        "spell" if draw_head == "spell" else "follower",
        True,
        0,
    )
    candidate = SourceCandidate(raw, descriptor, engine, context)
    derived = derive_source(raw, descriptor, engine, context=context)
    part = derived.field.parts[0]
    use = SourceUse.model_validate(
        {
            "binding_id": "bind:" + "0" * 64,
            "ability_scope": "ability_0",
            "occurrence": {
                "owner": descriptor.owner,
                "field": descriptor.field,
                "ordinal": descriptor.ordinal,
                "source_hash": descriptor.source_hash,
                "line_ordinal": part.line_ordinal,
                "role": part.source_span.role,
                "segments": part.source_span.segments,
            },
        }
    )
    entry = ability(head(name=draw_head)) | {"sources": [use.model_dump(mode="json")]}
    with pytest.raises(
        SourceError,
        match=r"source_pending|source_unresolved_issue|source_not_ability_body",
    ):
        expander.expand_sources(canonical(document(entry)), [], {PATH: candidate})


def context(draw_head: str) -> SourceContext:
    candidate, use, _ = draw_source(name=draw_head)
    return resolve_source(candidate, use)


@pytest.mark.parametrize(
    ("kind", "component", "owner", "path", "reason"),
    [
        ("Nat", "constant", "A.act.draw", ("n",), "parameter_source_role_mismatch"),
        ("Nat", "value", "A.act.damage", ("n",), "parameter_source_role_mismatch"),
        ("Player", "value", "A.act.draw", ("who",), "parameter_source_role_mismatch"),
        ("Int", "value", "A.act.draw", ("n",), "parameter_source_role_mismatch"),
    ],
)
def test_draw_operand_cannot_borrow_other_roles(
    kind: str,
    component: str,
    owner: str,
    path: tuple[str | int, ...],
    reason: str,
    draw_head: str,
) -> None:
    declaration = ParamDecl.model_validate(
        {"type": kind, "source": {"slot": "leaf_0", "component": component}}
    )
    with pytest.raises(SourceError, match=reason):
        parameter_expected(
            context(draw_head), declaration, owner, path, {"n": {"param": "n"}}
        )


@pytest.mark.parametrize(
    "change", ["role", "type", "optional", "extra", "occurrence", "value"]
)
def test_draw_payload_requires_one_exact_count_operand(
    change: str, draw_head: str
) -> None:
    source = context(draw_head)
    slot = source.frame.leaf_schema.slots[0]
    binding = source.binding
    slots: tuple[LeafSlot, ...] = (slot,)
    if change == "extra":
        slots = (slot, slot.model_copy(update={"name": "other"}))
    elif change == "occurrence":
        binding = binding.model_copy(update={"occurrences": ()})
    elif change == "value":
        binding = binding.model_copy(update={"values": {"leaf_0": True}})
    else:
        slots = (
            slot.model_copy(
                update={"role": "damage_amount"}
                if change == "role"
                else {"type": "Int"}
                if change == "type"
                else {"required": False}
            ),
        )
    source = replace(
        source,
        frame=source.frame.model_copy(
            update={
                "leaf_schema": source.frame.leaf_schema.model_copy(
                    update={"slots": slots}
                )
            }
        ),
        binding=binding,
    )
    with pytest.raises(SourceError, match="source_draw_operand"):
        entry_payload(source, "ability_0")


def test_draw_source_parameter_value_is_checked(
    expander: AuthorExpander, draw_head: str
) -> None:
    candidate, use, frame = draw_source(name=draw_head)
    entry, bodies = draw_ability(frame, use, companion=True, count=2, name=draw_head)
    with pytest.raises(ExpansionError, match="source_param_value_mismatch"):
        expander.expand_sources(canonical(document(entry)), bodies, {PATH: candidate})


def test_draw_does_not_parameterize_actor(
    expander: AuthorExpander, draw_head: str
) -> None:
    candidate, use, frame = draw_source(name=draw_head)
    record = registered(
        {draw_head: {"do": [{"draw": {"who": {"param": "who"}, "n": 3}}]}}
    )
    record["frame_id"] = frame.id
    record["params"] = {
        "who": {"type": "Player", "source": {"slot": "leaf_0", "component": "value"}}
    }
    record["body_version"] = body_version(record)
    entry = generated(record, {"who": {"type": "Player", "value": "self"}})
    entry["sources"] = [use.model_dump(mode="json")]
    with pytest.raises(SourceError, match="parameter_source_role_mismatch"):
        expander.expand_sources(
            canonical(document(entry)), [canonical(shard(record))], {PATH: candidate}
        )


def test_draw_source_scope_is_exact(draw_head: str) -> None:
    with pytest.raises(SourceError, match="source_ability_scope_mismatch"):
        entry_payload(context(draw_head), "ability_1")


def test_draw_cannot_add_a_receipt_port(
    expander: AuthorExpander, draw_head: str
) -> None:
    candidate, use, frame = draw_source(name=draw_head)
    entry, bodies = draw_ability(frame, use, companion=False, name=draw_head)
    entry[draw_head] = {"do": [{"draw": {"who": "self", "n": 3, "result": "drawn"}}]}
    entry["ports"] = {
        "imports": {},
        "exports": {"receipt": {"path": [draw_head, "do", 0, "draw"], "bind": "drawn"}},
    }
    raw = canonical(document(entry))
    expander.expand(raw, bodies)
    with pytest.raises(ExpansionError, match="unsupported_port_source_mapping"):
        expander.expand_sources(raw, bodies, {PATH: candidate})
