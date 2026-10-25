"""Self movement has a fixed head, PP payment, subject and owner destination."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.author_leaves import ParamDecl
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
    entry_heads,
    entry_payload,
    parameter_expected,
    resolve_source,
)

from ..translations.test_four_layer_fixed_actions import MOVE_RAW
from .author_move_cases import PATH, head, move_ability, move_source
from .test_author_entries import SCHEMAS, document, shard
from .test_author_expansion import generated, registered

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_sources import SourceContext


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


def context() -> SourceContext:
    candidate, use, _ = move_source()
    return resolve_source(candidate, use)


@pytest.mark.parametrize("companion", [False, True])
def test_fixed_self_move_expansion(
    expander: AuthorExpander, *, companion: bool
) -> None:
    candidate, use, frame = move_source()
    entry, bodies = move_ability(frame, use, companion=companion)
    result = expander.expand_sources(
        canonical(document(entry)), bodies, {PATH: candidate}
    )
    assert entry_heads(frame) == ("activate",)
    assert equivalent_ability(next(ability_nodes(result.tree))[1]) == canonical(
        head() | {"ports": {"imports": {}, "exports": {}}}
    )
    assert result.sidecar[0].source_proof == "derived_self_consistent"
    assert result.sidecar[0].frame_id == frame.id


ACTIVATE_CHANGES: dict[str, JsonValue] = {
    "condition": {"usable_if": {"cmp": {"left": 1, "op": "eq", "right": 1}}},
    "zone": {"zone": "hand"},
    "per-turn": {"per_turn": 1},
    "pp": {"cost": {"pp": 2}},
    "act": {"cost": {"steps": [{"pp": 1}, {"act_self": True}]}},
    "sacrifice": {"cost": {"steps": [{"pp": 1}, {"self_to_cemetery": True}]}},
    "destroy": {"do": [{"destroy": "this"}]},
    "quick": {"quick": True},
    "ub": {"ub": True},
    "advance": {"advance": True},
}


def bad_head(change: str) -> dict[str, JsonValue]:
    payload = head()
    activate = object_value(payload["activate"])
    move = object_value(object_value(array(activate["do"])[0])["move"])
    if change == "head":
        return {"spell": {"do": activate["do"]}}
    if change == "target":
        move["ref"] = {"who": "self", "zone": "field"}
    elif change == "destination":
        move["to"] = "hand"
    elif change == "by":
        move["by"] = {"master_of": "this"}
    elif change == "dynamic":
        activate["define"] = {"X": 1}
        activate["do"] = [{"draw": {"n": "X"}}, {"move": move}]
    elif change == "extra-step":
        activate["do"] = [{"move": move}, {"draw": {"n": 1}}]
    elif change == "implicit-zone":
        del activate["zone"]
    else:
        activate.update(object_value(ACTIVATE_CHANGES[change]))
    return payload


@pytest.mark.parametrize(
    "change",
    [
        "head",
        "target",
        "destination",
        "destroy",
        "dynamic",
        "condition",
        "pp",
        "act",
        "sacrifice",
        "extra-step",
        "implicit-zone",
        "zone",
        "by",
        "per-turn",
        "quick",
        "ub",
        "advance",
    ],
)
@pytest.mark.parametrize("companion", [False, True])
def test_complete_payload_is_fixed(
    expander: AuthorExpander,
    change: str,
    *,
    companion: bool,
) -> None:
    candidate, use, frame = move_source()
    entry, bodies = move_ability(
        frame, use, companion=companion, payload=bad_head(change)
    )
    raw = canonical(document(entry))
    expander.expand(raw, bodies)
    with pytest.raises(ExpansionError, match="source_head_payload_mismatch"):
        expander.expand_sources(raw, bodies, {PATH: candidate})


@pytest.mark.parametrize(
    "raw",
    [
        MOVE_RAW.replace("{起動}", "{ファンファーレ}"),
        MOVE_RAW.replace("これを墓場", "自分のフォロワーすべてを墓場"),
        MOVE_RAW.replace("墓場", "手札"),
        MOVE_RAW.replace("コスト1", "コスト2"),
        MOVE_RAW.replace("コスト1", "コストX"),
        MOVE_RAW.replace("：", "これを{アクト}："),
        MOVE_RAW.replace("：これを墓場に置く。", "これを墓場に置く：1枚引く。"),
        MOVE_RAW.replace("：これ", "：自分の手札が1枚なら、これ"),
        MOVE_RAW.split("\n")[1],
        MOVE_RAW.replace("1枚引く", "2枚引く"),
        MOVE_RAW + "1枚引く。",
    ],
    ids=[
        "head",
        "scope",
        "zone",
        "pp",
        "dynamic",
        "extra-cost",
        "payment",
        "condition",
        "missing-sibling",
        "changed-sibling",
        "extra-step",
    ],
)
def test_related_sources_stay_rejected(expander: AuthorExpander, raw: str) -> None:
    candidate, use, frame = move_source(raw)
    entry, bodies = move_ability(frame, use, companion=False)
    with pytest.raises(SourceError, match=r"source_pending|source_unresolved_issue"):
        expander.expand_sources(canonical(document(entry)), bodies, {PATH: candidate})


@pytest.mark.parametrize("change", ["owner", "binding", "scope", "line", "segments"])
def test_source_use_is_exact(expander: AuthorExpander, change: str) -> None:
    candidate, use, frame = move_source()
    tree = use.model_dump(mode="json")
    occurrence = object_value(tree["occurrence"])
    if change == "owner":
        occurrence["owner"] = {"kind": "face_revision", "revision_id": "revision:wrong"}
    elif change == "binding":
        tree["binding_id"] = "bind:" + "f" * 64
    elif change == "scope":
        tree["ability_scope"] = "ability_1"
    elif change == "line":
        occurrence["line_ordinal"] = 0
    else:
        occurrence["segments"] = [{"start": 29, "end": 48}]
    entry, bodies = move_ability(frame, use, companion=False)
    entry["sources"] = [tree]
    with pytest.raises(
        SourceError, match=r"source_occurrence_mismatch|source_ability_scope_mismatch"
    ):
        expander.expand_sources(canonical(document(entry)), bodies, {PATH: candidate})


@pytest.mark.parametrize("slot", [0, 1, 2])
@pytest.mark.parametrize(
    "change", ["type", "role", "domain", "optional", "occurrence", "value"]
)
def test_fixed_leaf_contract_cannot_drift(slot: int, change: str) -> None:
    source = context()
    slots = list(source.frame.leaf_schema.slots)
    old = slots[slot]
    binding = source.binding
    if change == "value":
        values = dict(binding.values)
        values[old.name] = True if slot == 1 else ("hand",)
        binding = binding.model_copy(update={"values": values})
    else:
        updates: dict[str, object] = {
            "type": "Int",
            "role": "count",
            "domain": old.domain.model_copy(update={"max": 2}),
            "required": False,
            "occurrences": (),
        }
        key = {"optional": "required", "occurrence": "occurrences"}.get(change, change)
        slots[slot] = old.model_copy(update={key: updates[key]})
    changed = replace(
        source,
        frame=source.frame.model_copy(
            update={
                "leaf_schema": source.frame.leaf_schema.model_copy(
                    update={"slots": tuple(slots)}
                )
            }
        ),
        binding=binding,
    )
    with pytest.raises(SourceError, match="source_move_contract_mismatch"):
        entry_payload(changed, "ability_0")


@pytest.mark.parametrize(
    ("kind", "slot", "owner", "path"),
    [
        ("Nat", "leaf_1", "A.effect.cost-mod", ("set",)),
        ("Zone", "leaf_2", "A.act.move.ref", ("to",)),
        ("Concept", "leaf_0", "A.head.activate", ("head",)),
    ],
)
def test_no_move_leaf_is_parameterized(
    kind: str, slot: str, owner: str, path: tuple[str, ...]
) -> None:
    declaration = ParamDecl.model_validate(
        {"type": kind, "source": {"slot": slot, "component": "value"}}
    )
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        parameter_expected(context(), declaration, owner, path, {})


def test_companion_cannot_parameterize_destination(expander: AuthorExpander) -> None:
    candidate, use, frame = move_source()
    payload = head()
    object_value(payload["activate"])["do"] = [
        {"move": {"ref": "this", "to": {"param": "zone"}}}
    ]
    record = registered(payload)
    record["frame_id"] = frame.id
    record["params"] = {
        "zone": {"type": "Zone", "source": {"slot": "leaf_2", "component": "value"}}
    }
    record["body_version"] = body_version(record)
    entry = generated(record, {"zone": {"type": "Zone", "value": "cemetery"}})
    entry["sources"] = [use.model_dump(mode="json")]
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        expander.expand_sources(
            canonical(document(entry)), [canonical(shard(record))], {PATH: candidate}
        )
