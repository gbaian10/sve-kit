"""Independent cv-v2 and render-v3 vectors pin the public identity recipes."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.annotations import AnnotationSet
from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_identities import (
    ContextVariant,
    context_variant,
)
from sve_carddb.domains.translations.four_layer_render import Rendered

from .test_four_layer_render import sample, target


def test_independent_context_variant_vector() -> None:
    variant = ContextVariant.model_validate_json(
        canonical(
            {
                "assignment": "default",
                "bindings": [
                    {
                        "frame_id": "frame:" + "a" * 64,
                        "values": {"n": 2},
                        "source_span": {
                            "role": "body",
                            "segments": [{"start": 0, "end": 4}],
                            "anchor": None,
                        },
                    }
                ],
            }
        )
    )
    assert (
        variant.identity()
        == "cv:3f8a7b2403bb791875bad192ce82fcfbfb4346f9fc35686a9f2008d27bb970f0"
    )
    assert (
        variant.model_copy(update={"assignment": "named"}).identity()
        != variant.identity()
    )


def test_independent_render_identity_vector() -> None:
    data = {
        "recipe": "annotation-v1",
        "text_unit_id": "t:zh-Hant:" + digest("A😀2".encode())[7:23],
        "occurrences": [],
    }
    annotation = AnnotationSet.model_validate(
        {
            "id": "ann:" + hash_payload(data),
            "text_unit_id": data["text_unit_id"],
            "occurrences": (),
        }
    )
    rendered = Rendered(
        "ctx:synthetic-n0",
        "zh-Hant",
        "A😀2",
        annotation,
        "2" * 64,
        "project",
        False,
        (),
    )
    assert rendered.identity() == (
        "tr:03f5c296d88e4635e327246ebd4b2cb02b5ecf1c185c9b6d4d0657a0ce972f26",
        69665064585444,
    )
    assert replace(rendered, low_confidence=True).identity() != rendered.identity()


def test_context_aggregation_sorts_source_ordinals_and_rejects_foreign_owners() -> None:
    first = sample(target([{"kind": "Literal", "text": "TEST"}])).binding
    second = first.model_copy(update={"ordinal": 1})
    assert context_variant((first, second)) == context_variant((second, first))
    assert context_variant((first,), "named") != context_variant((first,))
    with pytest.raises(ValueError, match="ordinals"):
        context_variant((first, first))
    with pytest.raises(ValueError, match="ordinals"):
        context_variant((second,))
    foreign = second.model_copy(
        update={
            "source": second.source.model_copy(
                update={"field": "section", "ordinal": 0}
            )
        }
    )
    with pytest.raises(ValueError, match="owner field"):
        context_variant((first, foreign))


def test_unknown_payload_keys_cannot_enter_the_variant_recipe() -> None:
    with pytest.raises(ValueError, match="Extra inputs"):
        ContextVariant.model_validate_json(
            canonical(
                {"assignment": "default", "bindings": [], "legacy_template": "T0"}
            )
        )
