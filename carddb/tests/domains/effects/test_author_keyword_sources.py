"""Keyword expectations retain complete typed sources before author admission."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    Domain,
    FaceRevisionOwner,
    GlossaryReference,
    Port,
    Scope,
    Span,
    hash_payload,
)
from sve_carddb.domains.effects.author_sources import (
    SourceContext,
    SourceError,
    card_keyword_values,
)
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from ..translations.test_four_layer_classification import classifier, source

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding


TERMS = (
    Term("term:keyword.ward", "keyword", "守護"),
    Term("term:keyword.aura", "keyword", "オーラ"),
    Term("term:keyword.storm", "keyword", "疾走"),
    Term("term:keyword.other", "keyword", "仮語"),
)


def context(raw: str = "【守護】【オーラ】") -> SourceContext:
    engine = classifier(TERMS, ("bracket_keyword_reference",))
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify_binding(raw, engine, frame, binding)
    return SourceContext(
        frame, binding, derived.field.parts[0].canonical_source, engine
    )


def reseal(
    original: SourceContext, frame: Frame, binding: SourceBinding
) -> SourceContext:
    content_hash = hash_payload(frame.payload(original.canonical_source))
    frame = frame.model_copy(
        update={"id": "frame:" + content_hash, "content_hash": content_hash}
    )
    binding = binding.model_copy(update={"frame_id": frame.id})
    binding = binding.model_copy(
        update={"id": "bind:" + hash_payload(binding.payload())}
    )
    return replace(original, frame=frame, binding=binding)


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("【守護】", ("ward",)),
        ("【オーラ】", ("aura",)),
        ("【守護】【オーラ】", ("ward", "aura")),
        ("【オーラ】【守護】", ("aura", "ward")),
    ],
    ids=("ward", "aura", "ward-aura", "aura-ward"),
)
def test_complete_keyword_order_is_printed_order(
    raw: str, expected: tuple[str, ...]
) -> None:
    found = context(raw)
    assert card_keyword_values(found) == expected
    assert found.source_proof == "none"


@pytest.mark.parametrize(
    "raw",
    ["【疾走】", "【守護】【疾走】", "【仮語】"],
    ids=("storm", "mixed", "unknown"),
)
def test_registered_concept_does_not_imply_author_mapping(raw: str) -> None:
    with pytest.raises(SourceError, match="unsupported_source_keyword"):
        card_keyword_values(context(raw))


def test_repeated_source_keyword_is_never_deduplicated() -> None:
    with pytest.raises(SourceError, match="unsupported_duplicate_source_keyword"):
        card_keyword_values(context("【守護】【守護】"))


@pytest.mark.parametrize(
    "change",
    [
        "type",
        "role",
        "required",
        "domain",
        "missing-value",
        "swapped-value",
        "extra-value",
        "occurrences",
        "occurrence-order",
        "frame",
        "discriminator",
        "scope",
        "ports",
        "field",
        "trace",
    ],
)
def test_complete_keyword_contract_rejects_tampering(change: str) -> None:
    original = context()
    frame, binding = original.frame, original.binding
    slots = list(frame.leaf_schema.slots)
    if change in {"type", "role", "required", "domain"}:
        slot_changes: dict[str, dict[str, object]] = {
            "type": {"type": "CardName"},
            "role": {"role": "ability"},
            "required": {"required": False},
            "domain": {
                "domain": Domain(values=("concept.ability.v1",), min=None, max=None)
            },
        }
        slots[0] = slots[0].model_copy(update=slot_changes[change])
        frame = frame.model_copy(
            update={
                "leaf_schema": frame.leaf_schema.model_copy(
                    update={"slots": tuple(slots)}
                )
            }
        )
    elif change in {"missing-value", "swapped-value", "extra-value"}:
        values = dict(binding.values)
        if change == "missing-value":
            values.pop("leaf_1")
        elif change == "swapped-value":
            values["leaf_1"] = GlossaryReference(
                kind="glossary", key="term:keyword.storm"
            )
        else:
            values["extra"] = values["leaf_0"]
        binding = binding.model_copy(update={"values": values})
    elif change in {"occurrences", "occurrence-order"}:
        occurrences = (
            binding.occurrences[:1]
            if change == "occurrences"
            else tuple(reversed(binding.occurrences))
        )
        binding = binding.model_copy(update={"occurrences": occurrences})
    elif change == "frame":
        binding = binding.model_copy(update={"frame_id": "frame:" + "f" * 64})
        with pytest.raises(SourceError):
            card_keyword_values(replace(original, binding=binding))
        return
    elif change in {"discriminator", "scope", "ports"}:
        projection_changes: dict[str, dict[str, object]] = {
            "discriminator": {"discriminator": "other.v1"},
            "scope": {"scopes": (Scope(id="ability_0", parent=None, kind="ability"),)},
            "ports": {
                "scopes": (Scope(id="ability_0", parent=None, kind="ability"),),
                "exports": (
                    Port(
                        name="p",
                        type="QuantityExpr",
                        source_role="keyword",
                        scope="ability_0",
                    ),
                ),
            },
        }
        frame = frame.model_copy(
            update={
                "projection": frame.projection.model_copy(
                    update=projection_changes[change]
                )
            }
        )
    elif change == "field":
        binding = binding.model_copy(
            update={"source": binding.source.model_copy(update={"field": "name"})}
        )
    else:
        binding = binding.model_copy(update={"trace": ()})
    with pytest.raises(SourceError):
        card_keyword_values(reseal(original, frame, binding))


@pytest.mark.parametrize(
    "change",
    [
        "owner",
        "field",
        "ordinal",
        "source-hash",
        "line",
        "segments",
        "trace",
        "swapped-supported-values",
        "omitted",
        "resolution",
        "extra-span",
    ],
)
def test_resealed_keyword_binding_requires_independent_source_reconstruction(
    change: str,
) -> None:
    raw = "【守護】【オーラ】"
    original = context(raw)
    engine = original.classifier
    assert engine is not None
    derived = derive_source(raw, source(raw), engine)
    binding = original.binding
    if change in {"owner", "field", "source-hash"}:
        descriptor = binding.source
        if change == "owner":
            descriptor = descriptor.model_copy(
                update={
                    "owner": FaceRevisionOwner(
                        kind="face_revision", revision_id="revision:foreign"
                    )
                }
            )
        elif change == "field":
            descriptor = descriptor.model_copy(
                update={"field": "section", "ordinal": 0}
            )
        else:
            descriptor = descriptor.model_copy(
                update={
                    "source_hash": "f" * 64,
                    "source_ref": descriptor.source_ref.model_copy(
                        update={"text_hash": "f" * 64}
                    ),
                }
            )
        binding = binding.model_copy(update={"source": descriptor})
    elif change in {"ordinal", "line"}:
        binding = binding.model_copy(
            update={"ordinal" if change == "ordinal" else "line_ordinal": 1}
        )
    elif change == "segments":
        binding = binding.model_copy(
            update={
                "source_span": binding.source_span.model_copy(
                    update={"segments": (Span(start=1, end=9),)}
                )
            }
        )
    elif change == "trace":
        trace = (
            binding.trace[0].model_copy(update={"rule": "forged"}),
            *binding.trace[1:],
        )
        binding = binding.model_copy(update={"trace": trace})
    elif change == "swapped-supported-values":
        binding = binding.model_copy(
            update={
                "values": {
                    "leaf_0": binding.values["leaf_1"],
                    "leaf_1": binding.values["leaf_0"],
                }
            }
        )
    else:
        occurrence = binding.occurrences[0]
        occurrence_changes: dict[str, dict[str, object]] = {
            "omitted": {
                "source_presence": "omitted",
                "raw_spans": (),
                "canonical_spans": (),
                "resolution_rule": "forged",
            },
            "resolution": {"resolution_rule": "forged"},
            "extra-span": {
                "canonical_spans": (Span(start=1, end=3), Span(start=5, end=8))
            },
        }
        binding = binding.model_copy(
            update={
                "occurrences": (
                    occurrence.model_copy(update=occurrence_changes[change]),
                    *binding.occurrences[1:],
                )
            }
        )
    forged = reseal(original, original.frame, binding)
    with pytest.raises(ValueError, match=r"Binding|Leaf|Omitted"):
        derived.verify_binding(raw, engine, forged.frame, forged.binding)
    if change in {"omitted", "resolution", "extra-span"}:
        with pytest.raises(SourceError):
            card_keyword_values(forged)


def test_resolved_quick_is_not_a_ward_aura_declaration() -> None:
    raw = "{クイック}"
    engine = classifier(
        (Term("term:ability.quick", "ability", "クイック"),),
        ("braced_ability_reference",),
    )
    descriptor = source(raw)
    facts = CardContext(
        descriptor,
        "c:synthetic",
        "f:synthetic",
        "normal",
        "spell",
        True,
        0,
        frozenset(),
    )
    derived = derive_source(raw, descriptor, engine, context=facts)
    frame, binding = derived.bind(0)
    assert frame.semantic_variant.key == "quick_card_field.v1"
    derived.verify_binding(raw, engine, frame, binding)
    with pytest.raises(SourceError, match="source_keyword_contract_mismatch"):
        card_keyword_values(
            SourceContext(
                frame, binding, derived.field.parts[0].canonical_source, engine
            )
        )


def test_registered_reminder_cannot_create_a_keyword_declaration() -> None:
    raw = "【疾走】（プレイしたターンから攻撃できる。）"
    engine = classifier(TERMS, ("bracket_keyword_reference",))
    derived = derive_source(raw, source(raw), engine)
    reminder = next(
        part for part in derived.field.parts if part.source_span.role == "reminder"
    )
    frame, binding = derived.bind(reminder.ordinal)
    assert frame.semantic_variant.key == "pure_reminder.v1"
    assert frame.projection.projection_kind == "none"
    derived.verify_binding(raw, engine, frame, binding)
    with pytest.raises(SourceError, match="source_keyword_contract_mismatch"):
        card_keyword_values(
            SourceContext(frame, binding, reminder.canonical_source, engine)
        )
