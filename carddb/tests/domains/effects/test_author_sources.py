"""Reconstruction cannot borrow synthetic resolved claims or equal-valued operands."""

import json
import re
from dataclasses import replace
from hashlib import sha256
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl, ParamSource
from sve_carddb.contracts.four_layer import Domain, LeafSchema, LeafSlot, Port
from sve_carddb.domains.effects.author_sources import (
    SourceCandidate,
    SourceError,
    parameter_expected,
    resolve_source,
)
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from ..translations.test_four_layer_classification import (
    classifier,
    source,
    type_vocabulary,
)
from .author_source_cases import synthetic_offset

if TYPE_CHECKING:
    from pydantic import JsonValue


def test_synthetic_compatibility_does_not_grant_source_proof() -> None:
    sample = synthetic_offset()
    compatible = sample.compatible()
    assert compatible.source_proof == "none"
    assert (
        parameter_expected(
            compatible,
            sample.declaration(),
            "A.expr.minus",
            ("minus", 1),
            {
                "minus": [
                    {"count": {"who": "self", "zone": "deck"}},
                    {"param": "offset"},
                ]
            },
        )
        == 5
    )
    with pytest.raises(SourceError, match="source_pending"):
        resolve_source(sample.candidate, sample.use())


@pytest.mark.parametrize(
    "fragment",
    [
        {"minus": [{"count": {"who": "opp", "zone": "deck"}}, {"param": "offset"}]},
        {"minus": [{"count": {"who": "self", "zone": "hand"}}, {"param": "offset"}]},
        {
            "minus": [
                {"count": {"who": "self", "zone": "deck", "kind": "spell"}},
                {"param": "offset"},
            ]
        },
        {
            "minus": [
                {"count": {"who": "self", "zone": {"param": "zone"}}},
                {"param": "offset"},
            ]
        },
        {"minus": [5, {"param": "offset"}]},
        {"plus": [{"count": {"who": "self", "zone": "deck"}}, {"param": "offset"}]},
        {"minus": [{"count": {"who": "self", "zone": "deck"}}, 5]},
        {"minus": [{"count": {"who": "self", "zone": "deck"}}]},
    ],
)
def test_offset_retains_complete_sibling(fragment: JsonValue) -> None:
    sample = synthetic_offset()
    with pytest.raises(SourceError, match="parameter_source_operand_mismatch"):
        parameter_expected(
            sample.compatible(),
            sample.declaration(),
            "A.expr.minus",
            ("minus", 1),
            fragment,
        )


@pytest.mark.parametrize(
    ("owner", "path"),
    [
        ("A.expr.minus", ("minus", 0)),
        ("A.expr.plus", ("plus", 1)),
        ("A.act.draw", ("n",)),
        ("A.instead.draw", ("n",)),
        ("A.expr.minus", ("minus", 1, "extra")),
    ],
)
def test_offset_does_not_borrow_other_sites(
    owner: str, path: tuple[str | int, ...]
) -> None:
    sample = synthetic_offset()
    with pytest.raises(SourceError):
        parameter_expected(
            sample.compatible(),
            sample.declaration(),
            owner,
            path,
            {
                "minus": [
                    {"count": {"who": "self", "zone": "deck"}},
                    {"param": "offset"},
                ]
            },
        )


def test_equal_value_slot_is_not_expression_operand() -> None:
    sample = synthetic_offset()
    original = next(
        s for s in sample.frame.leaf_schema.slots if s.role == "arithmetic_offset"
    )
    other = LeafSlot(
        name="other",
        type="Nat",
        role="arithmetic_offset",
        domain=Domain(values=(), min=0, max=99),
        required=True,
        occurrences=(),
    )
    context = sample.compatible()
    frame = context.frame.model_copy(
        update={
            "leaf_schema": LeafSchema(
                format=2, slots=(*context.frame.leaf_schema.slots, other)
            )
        }
    )
    binding = context.binding.model_copy(
        update={
            "values": {
                **context.binding.values,
                "other": context.binding.values[original.name],
            }
        }
    )
    altered = replace(context, frame=frame, binding=binding)
    declaration = ParamDecl(
        type="Int", source=ParamSource(slot="other", component="value")
    )
    with pytest.raises(SourceError, match="parameter_source_operand_mismatch"):
        parameter_expected(
            altered,
            declaration,
            "A.expr.minus",
            ("minus", 1),
            {"minus": [{"count": {"who": "self", "zone": "deck"}}, {"param": "other"}]},
        )


@pytest.mark.parametrize("kind", ["Concept", "CardKind", "Phase"])
def test_unregistered_mappings_fail_closed(kind: str) -> None:
    sample = synthetic_offset()
    declaration = ParamDecl.model_validate(
        {
            "type": kind,
            "source": {"slot": sample.declaration().source.slot, "component": "value"},
        }
    )
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        parameter_expected(
            sample.compatible(), declaration, "A.expr.minus", ("minus", 1), {}
        )


def test_negative_magnitude_never_borrows_a_minus_sign() -> None:
    sample = synthetic_offset()
    declaration = sample.declaration().model_copy(
        update={
            "source": sample.declaration().source.model_copy(
                update={"component": "negated_magnitude"}
            )
        }
    )
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        parameter_expected(
            sample.compatible(),
            declaration,
            "A.effect.cost-mod",
            ("delta",),
            {"delta": {"param": "offset"}},
        )


@pytest.mark.parametrize("direction", ["imports", "exports"])
def test_nonempty_projection_ports_fail_closed(direction: str) -> None:
    sample = synthetic_offset()
    projection = sample.frame.projection.model_copy(
        update={
            direction: (
                Port(
                    name="objects",
                    type="ObjectSet",
                    source_role="selection",
                    scope="ability_0",
                ),
            )
        }
    )
    sample = replace(
        sample, frame=sample.frame.model_copy(update={"projection": projection})
    )
    with pytest.raises(SourceError, match="unsupported_source_ports"):
        sample.compatible()


def test_compatibility_rechecks_hashes() -> None:
    sample = synthetic_offset()
    sample = replace(sample, canonical_source=sample.canonical_source + "x")
    with pytest.raises(SourceError, match="source_compatibility_mismatch"):
        sample.compatible()


def entry_case() -> tuple[SourceCandidate, SourceUse]:
    raw = "{進化}{コスト９９}:これは進化する。"
    descriptor = source(raw)
    engine = classifier()
    context = CardContext(
        descriptor, "card:synthetic", "face:normal", "normal", "follower"
    )
    candidate = SourceCandidate(raw, descriptor, engine, context)
    derived = derive_source(raw, descriptor, engine, context=context)
    _, binding = derived.bind(0)
    use = SourceUse(
        binding_id=binding.id,
        occurrence=binding.occurrence_key(),
        ability_scope="ability_0",
    )
    return candidate, use


def test_entry_reconstruction_has_limited_proof() -> None:
    candidate, use = entry_case()
    result = resolve_source(candidate, use)
    assert result.source_proof == "derived_self_consistent"
    assert result.frame.semantic_variant.key == "evolve_entry.v1"
    assert result.binding.id == use.binding_id
    assert result.classifier is candidate.classifier


@pytest.mark.parametrize("change", ["binding", "owner", "scope", "line", "raw"])
def test_exact_source_use_is_required(change: str) -> None:
    candidate, use = entry_case()
    if change == "binding":
        use = use.model_copy(update={"binding_id": "bind:" + "0" * 64})
    elif change == "owner":
        use = use.model_copy(
            update={
                "occurrence": use.occurrence.model_copy(
                    update={
                        "owner": use.occurrence.owner.model_copy(
                            update={"revision_id": "other"}
                        )
                    }
                )
            }
        )
    elif change == "scope":
        use = use.model_copy(update={"ability_scope": "unknown"})
    elif change == "line":
        use = use.model_copy(
            update={"occurrence": use.occurrence.model_copy(update={"line_ordinal": 9})}
        )
    else:
        candidate = replace(candidate, raw=candidate.raw + "x")
    with pytest.raises(SourceError):
        resolve_source(candidate, use)


@pytest.mark.parametrize(
    ("card_no", "pack", "line", "checksum", "reason"),
    [
        (
            "BP08-038",
            "BP08",
            1,
            "7a884eac36c1801277243b0bd7489062e396130b52be10bf3ab09ea4869904cb",
            "source_pending",
        ),
        (
            "CP04-060",
            "CP04",
            0,
            "86fa086090511c9105cfc1d9210f2102f69ef7351e6930dcd9dd89a8f53f3350",
            "source_unresolved_issue",
        ),
    ],
)
def test_frozen_real_cards_do_not_become_resolved(
    card_no: str, pack: str, line: int, checksum: str, reason: str
) -> None:
    path = (
        Path(__file__).resolve().parents[4] / "card-data" / "jp" / pack / "000-099.json"
    )
    data = json.loads(path.read_bytes())
    card = next(c for c in data["cards"] if c["card_no"] == card_no)
    texts = {
        document["record"]["faces"][0]["text"].split("\n")[line]
        for document in card["documents"].values()
    }
    # Bind intermediates so assertion rewriting never prints official wording.
    count = len(texts)
    assert count == 1
    raw = texts.pop()
    actual = sha256(raw.encode()).hexdigest()
    assert actual == checksum
    terms = tuple(
        Term("term:name.synthetic" + str(i), "card_name", name)
        for i, name in enumerate(dict.fromkeys(re.findall(r"『([^』]+)』", raw)))
    )
    engine = classifier(terms, vocabulary=type_vocabulary())
    candidate = SourceCandidate(raw, source(raw), engine)
    derived = derive_source(raw, candidate.descriptor, engine)
    part = derived.field.parts[0]
    use = SourceUse.model_validate(
        {
            "binding_id": "bind:" + "0" * 64,
            "ability_scope": "ability_0",
            "occurrence": {
                "owner": candidate.descriptor.owner,
                "field": candidate.descriptor.field,
                "ordinal": candidate.descriptor.ordinal,
                "source_hash": candidate.descriptor.source_hash,
                "line_ordinal": part.line_ordinal,
                "role": part.source_span.role,
                "segments": part.source_span.segments,
            },
        }
    )
    with pytest.raises(SourceError, match=reason):
        resolve_source(candidate, use)


@pytest.mark.parametrize("position", [0, 1])
def test_named_add_offset_uses_same_classifier_and_preserves_name(
    position: int,
) -> None:
    sample = synthetic_offset(named=True)
    operands: list[JsonValue] = [
        {"count": {"who": "self", "zone": "field", "name": "仮名"}}
    ]
    operands.insert(position, {"param": "offset"})
    assert (
        parameter_expected(
            sample.compatible(),
            sample.declaration(),
            "A.expr.plus",
            ("plus", position),
            {"plus": operands},
        )
        == 5
    )
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        parameter_expected(
            replace(sample.compatible(), classifier=None),
            sample.declaration(),
            "A.expr.plus",
            ("plus", position),
            {"plus": operands},
        )


def test_named_offset_rejects_other_name_with_same_number() -> None:
    sample = synthetic_offset(named=True)
    with pytest.raises(SourceError, match="parameter_source_operand_mismatch"):
        parameter_expected(
            sample.compatible(),
            sample.declaration(),
            "A.expr.plus",
            ("plus", 1),
            {
                "plus": [
                    {"count": {"who": "self", "zone": "field", "name": "別名"}},
                    {"param": "offset"},
                ]
            },
        )


def test_resolved_entry_cost_does_not_become_cost_modifier() -> None:
    candidate, use = entry_case()
    resolved = resolve_source(candidate, use)
    slot = next(s for s in resolved.frame.leaf_schema.slots if s.role == "cost_value")
    declaration = ParamDecl(
        type="Nat", source=ParamSource(slot=slot.name, component="value")
    )
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        parameter_expected(
            resolved,
            declaration,
            "A.effect.cost-mod",
            ("set",),
            {"set": {"param": "cost"}},
        )


def test_source_proof_cannot_be_passed_as_constructor_argument() -> None:
    sample = synthetic_offset()
    with pytest.raises(TypeError):
        # Exercise rejection of caller-supplied proof at runtime.
        type(sample.compatible())(  # type: ignore[call-arg]
            sample.frame,
            sample.binding,
            sample.canonical_source,
            source_proof="derived_self_consistent",
        )


def test_replacing_source_evidence_drops_reconstruction_proof() -> None:
    candidate, use = entry_case()
    resolved = resolve_source(candidate, use)
    assert replace(resolved, canonical_source="synthetic").source_proof == "none"
