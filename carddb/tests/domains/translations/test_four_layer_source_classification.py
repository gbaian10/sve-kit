"""Valid shape and hashes cannot substitute for independently classified source roles."""

import pytest

from sve_carddb.contracts.four_layer import (
    Constant,
    LeafSchema,
    QuantitySpec,
    hash_payload,
)
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source

from .test_four_layer_classification import classifier, source


@pytest.mark.parametrize("mutation", ["role", "mode", "unit"])
def test_source_boundary_rejects_validly_hashed_wrong_role_mode_or_unit(
    mutation: str,
) -> None:
    raw = "自分の手札のカード２枚まで選ぶ。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    engine = classifier()
    frame, binding = engine.recognize(raw, field.source, part).bind(field.source, part)
    engine.verify(raw, field, part, frame, binding)
    if mutation == "role":
        frame = frame.model_copy(
            update={
                "leaf_schema": LeafSchema(
                    format=2,
                    slots=(
                        frame.leaf_schema.slots[0].model_copy(
                            update={
                                "role": "repeat_count",
                                "domain": frame.leaf_schema.slots[0].domain.model_copy(
                                    update={
                                        "values": ("quantity.repeat_count.constant.v1",)
                                    }
                                ),
                            }
                        ),
                    ),
                )
            }
        )
        checksum = hash_payload(frame.payload(part.canonical_source))
        frame = frame.model_copy(
            update={"id": "frame:" + checksum, "content_hash": checksum}
        )
        binding = binding.model_copy(update={"frame_id": frame.id})
    elif mutation == "mode":
        binding = binding.model_copy(
            update={
                "values": {
                    "leaf_0": QuantitySpec(
                        mode="exact", expr=Constant(kind="constant", value=2)
                    )
                }
            }
        )
    else:
        binding = binding.model_copy(
            update={
                "occurrences": (
                    binding.occurrences[0].model_copy(update={"source_unit": "体"}),
                )
            }
        )
    binding = binding.model_copy(
        update={"id": "bind:" + hash_payload(binding.payload())}
    )
    part.verify(raw, frame, binding, engine.domains)
    with pytest.raises(ValueError, match="differs from"):
        engine.verify(raw, field, part, frame, binding)
