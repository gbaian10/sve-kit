"""Production v4 reconstructs source evidence and rejects unsupported versions."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.contracts.four_layer import (
    Domain,
    LeafSchema,
    LeafSlot,
    Port,
    Scope,
    Span,
    hash_payload,
)
from sve_carddb.contracts.source_binding import LeafOccurrence
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_analysis import analyze_source
from sve_carddb.domains.translations.four_layer_authored import from_files
from sve_carddb.domains.translations.four_layer_classification import Recognized
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_matching import bind_recognized
from sve_carddb.domains.translations.four_layer_normalizer import VERSION

from ...support.semantic_definition_fixtures import with_definitions
from .test_four_layer_classification import classifier, source
from .test_four_layer_n1 import n1_classifier

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame


def test_preparation_preserves_existing_pure_omitted_owner_bindings() -> None:
    raw = "自分の墓場のフォロワー７枚を選び、手札に加える。"
    engine = n1_classifier()
    prepared = derive_source(raw, source(raw), engine)
    frame, binding = prepared.bind(0)
    assert any(o.source_presence == "omitted" for o in binding.occurrences)
    prepared.verify(raw, engine, frame, binding)


def test_production_reconstructs_v4_from_complete_source() -> None:
    raw = "自分の手札のカード２枚を選ぶ。"
    engine = classifier()
    prepared = derive_source(raw, source(raw), engine)
    frame, binding = prepared.bind(0)
    assert frame.source.normalizer_version == VERSION
    prepared.verify(raw, engine, frame, binding)
    engine.verify(raw, prepared.field, prepared.field.parts[0], frame, binding)
    with pytest.raises(ValueError, match="outside"):
        prepared.verify(raw, engine, frame, binding.model_copy(update={"ordinal": 99}))
    changed = replace(prepared, derived=replace(prepared.derived, recognized=()))
    with pytest.raises(ValueError, match="reconstructed"):
        changed.verify(raw, engine, frame, binding)
    with pytest.raises(ValueError, match="construction"):
        prepared.verify(
            raw,
            engine,
            frame.model_copy(
                update={
                    "projection": frame.projection.model_copy(
                        update={
                            "scopes": (
                                Scope(id="ability", parent=None, kind="ability"),
                            )
                        }
                    )
                }
            ),
            binding,
        )


@pytest.mark.parametrize(
    "version",
    ["four-layer-jp-v2", "four-layer-jp-v4", "four-layer-jp-v3", "four-layer-jp-v999"],
)
def test_production_authored_and_binding_entry_reject_preparation_versions(
    version: str,
) -> None:
    raw = "自分の手札のカード２枚を選ぶ。"
    old = analyze_source(raw, source(raw), classifier())
    found = old.recognized[0]
    with pytest.raises(ValueError, match="Unsupported"):
        found.bind(old.field.source, old.field.parts[0], normalizer_version=version)
    frame, _ = old.recognized[0].bind(old.field.source, old.field.parts[0])
    frame = frame.model_copy(
        update={
            "source": frame.source.model_copy(update={"normalizer_version": version})
        }
    )
    content = canonical(
        {
            "format": 3,
            "kind": "translation_shard",
            "records": [
                {"kind": "sentence_template", "data": frame.model_dump(mode="json")},
            ],
        }
    )
    with pytest.raises(ValueError, match="normalizer version"):
        from_files(
            with_definitions(
                (("translations/templates/definitions/001.yaml", content, content),)
            )
        )
    if version != VERSION:
        with pytest.raises(ValueError, match="Unsupported"):
            found.bind(old.field.source, old.field.parts[0], normalizer_version=version)


def test_choice_context_is_only_a_port_with_the_existing_four_fields() -> None:
    port = Port(
        name="choice",
        type="ChoiceContext",
        source_role="choice_mode_count",
        scope="ability",
    )
    assert set(port.model_dump()) == {"name", "type", "source_role", "scope"}
    with pytest.raises(ValidationError):
        Port.model_validate(port.model_dump() | {"modes": 2})
    with pytest.raises(ValidationError):
        LeafSlot.model_validate(
            {
                "name": "choice",
                "type": "ChoiceContext",
                "role": "choice",
                "domain": {"values": [], "min": None, "max": None},
                "required": True,
                "occurrences": [],
            }
        )


def mixed_case() -> tuple[Recognized, Frame]:
    raw = "甲乙"
    prepared = derive_source(raw, source(raw), classifier())
    frame, _ = prepared.bind(0)
    slots = tuple(
        LeafSlot(
            name=name,
            type="Player",
            role="source_owner",
            domain=Domain(values=("player.self.v1",), min=None, max=None),
            required=True,
            occurrences=(Span(start=index, end=index + 1),),
        )
        for index, name in enumerate(("first", "second"))
    )
    occurrences = tuple(
        LeafOccurrence(
            slot=slot.name,
            ordinal=0,
            raw_spans=slot.occurrences,
            canonical_spans=slot.occurrences,
            source_unit=None,
            source_presence="explicit",
            resolution_rule=None,
        )
        for slot in slots
    ) + tuple(
        LeafOccurrence(
            slot=slot.name,
            ordinal=1,
            raw_spans=(),
            canonical_spans=(),
            source_unit=None,
            source_presence="omitted",
            resolution_rule="synthetic.same_operand.v1",
        )
        for slot in slots
    )
    found = Recognized(
        LeafSchema(format=2, slots=slots),
        {s.name: "self" for s in slots},
        occurrences,
        (),
        False,
    )
    frame = frame.model_copy(update={"leaf_schema": found.schema})
    checksum = hash_payload(frame.payload(raw))
    return found, frame.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )


def test_mixed_occurrences_follow_explicit_operand_mapping_even_when_values_match() -> (
    None
):
    found, frame = mixed_case()
    prepared = derive_source("甲乙", source("甲乙"), classifier())
    field = prepared.derived.field
    renamed = frame.model_copy(
        update={
            "leaf_schema": LeafSchema(
                format=2,
                slots=tuple(
                    slot.model_copy(update={"name": "actual_" + slot.name})
                    for slot in frame.leaf_schema.slots
                ),
            )
        }
    )
    checksum = hash_payload(renamed.payload("甲乙"))
    renamed = renamed.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    binding = bind_recognized(renamed, field, field.parts[0], found)
    assert binding is not None
    field.parts[0].verify(
        "甲乙",
        renamed,
        binding,
        classifier().domains,
        normalizer_version=VERSION,
    )
    assert [o.slot for o in binding.occurrences] == [
        "actual_first",
        "actual_second",
        "actual_first",
        "actual_second",
    ]
    wrong = renamed.model_copy(
        update={
            "leaf_schema": LeafSchema(
                format=2,
                slots=(
                    renamed.leaf_schema.slots[0].model_copy(
                        update={"role": "counted_owner"}
                    ),
                    renamed.leaf_schema.slots[1],
                ),
            )
        }
    )
    assert bind_recognized(wrong, field, field.parts[0], found) is None
    ambiguous = replace(
        found,
        schema=LeafSchema(
            format=2,
            slots=(
                found.schema.slots[0].model_copy(
                    update={
                        "occurrences": tuple(
                            span
                            for slot in found.schema.slots
                            for span in slot.occurrences
                        ),
                    }
                ),
            ),
        ),
        values={"first": "self"},
        occurrences=tuple(
            occurrence.model_copy(update={"slot": "first", "ordinal": index})
            for index, occurrence in enumerate(found.occurrences[:3])
        ),
    )
    assert bind_recognized(renamed, field, field.parts[0], ambiguous) is None
    # Matching synthetic evidence cannot authorize an unsealed source relation.
    with pytest.raises(ValueError, match="complete current source construction"):
        prepared.verify("甲乙", classifier(), renamed, binding)


def test_production_authored_admits_typed_choice_context_ports() -> None:
    raw = "自分の手札のカード２枚を選ぶ。"
    derived = analyze_source(raw, source(raw), classifier())
    frame, _ = derived.recognized[0].bind(derived.field.source, derived.field.parts[0])
    frame = frame.model_copy(
        update={
            "projection": frame.projection.model_copy(
                update={
                    "scopes": (Scope(id="ability", parent=None, kind="ability"),),
                    "exports": (
                        Port(
                            name="choice",
                            type="ChoiceContext",
                            source_role="choice_mode_count",
                            scope="ability",
                        ),
                    ),
                }
            )
        }
    )
    content = canonical(
        {
            "format": 3,
            "kind": "translation_shard",
            "records": [
                {"kind": "sentence_template", "data": frame.model_dump(mode="json")}
            ],
        }
    )
    assert from_files(
        with_definitions(
            (("translations/templates/definitions/001.yaml", content, content),)
        )
    ).records
