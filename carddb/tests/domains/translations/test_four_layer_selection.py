"""Explicit source selection keeps reusable wording separate from source semantics."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    Inputs,
    MatchRecord,
    OverrideRecord,
    TargetRecord,
    TargetVariantRecord,
    TemplateTarget,
)
from sve_carddb.domains.translations.four_layer_fields import render_field
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_render import Renderer, SelectedTarget
from sve_carddb.domains.translations.four_layer_selection import Controls
from sve_carddb.domains.translations.four_layer_storage import read_binding, read_target

from .test_four_layer_fields import _compiled
from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared SQLite schema fixture
from .test_four_layer_storage import stored as stored  # ruff: ignore[useless-import-alias] -- shared exact source fixture

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build import Database
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.translations.four_layer_authored import Record


def controls(*records: Record) -> Controls:
    return Controls(Inputs((), records))


def manual(binding: SourceBinding, frame: Frame) -> MatchRecord:
    return MatchRecord.model_validate_json(
        canonical(
            {
                "kind": "template_match",
                "data": {
                    "context_key": {
                        "source_unit_id": binding.source.source_unit_id,
                        "variant": "default",
                    },
                    "source_hash": "sha256:" + binding.source.source_hash,
                    "matches": [
                        {
                            "frame_id": frame.id,
                            "source_span": binding.source_span.model_dump(mode="json"),
                            "values": binding.model_dump(mode="json")["values"],
                        }
                    ],
                },
            }
        )
    )


def override(
    binding: SourceBinding, action: str, templates: list[JsonValue]
) -> OverrideRecord:
    return OverrideRecord.model_validate_json(
        canonical(
            {
                "kind": "translation_override",
                "data": {
                    "context_key": {
                        "source_unit_id": binding.source.source_unit_id,
                        "variant": "default",
                    },
                    "lang": "zh-Hant",
                    "action": action,
                    "templates": templates,
                    "terms": [],
                    "reason": "Synthetic wording selection",
                },
            }
        )
    )


def test_manual_match_uses_actual_verified_values_and_rejects_stale_or_partial_selection(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    field = _compiled(db, frame, binding)
    record = manual(binding, frame)
    selected = controls(FrameRecord(kind="sentence_template", data=frame), record)
    selected.frames_for(
        binding.source.source_unit_id, binding.source.source_hash, Frames((frame,))
    )
    selected.verify(field)
    selected.verify_closure()
    with pytest.raises(ValueError, match="stale source"):
        selected.frames_for(binding.source.source_unit_id, "0" * 64, Frames((frame,)))
    partial = record.model_copy(
        update={"data": record.data.model_copy(update={"matches": ()})}
    )
    with pytest.raises(ValueError, match="complete exact field"):
        controls(partial).verify(field)
    assert record.data.matches is not None
    altered = record.data.matches[0].model_copy(update={"values": {}})
    wrong = record.model_copy(
        update={"data": record.data.model_copy(update={"matches": (altered,)})}
    )
    with pytest.raises(ValueError, match="verified source leaves"):
        controls(wrong).verify(field)
    with pytest.raises(ValueError, match="exact current source"):
        controls(record).verify_closure()


@pytest.mark.parametrize("action", ["default", "suppress", "pin"])
def test_selected_wording_is_visible_only_after_a_context_override(
    stored: tuple[Database, Frame, SourceBinding],
    action: str,
) -> None:
    db, frame, binding = stored
    field = _compiled(db, frame, binding)
    target = read_target(db, frame.id, "zh-Hant", "default")
    named = TargetVariantRecord.model_validate_json(
        canonical(
            {
                "kind": "template_translation_variant",
                "origin": "machine",
                "low_confidence": True,
                "data": {
                    "template_id": frame.id,
                    "lang": "zh-Hant",
                    "variant_key": "alternative",
                    "target": {
                        "format": 1,
                        "nodes": [{"kind": "Literal", "text": "其他合成譯法"}],
                    },
                },
            }
        )
    )
    pins: list[JsonValue] = (
        [{"template_id": frame.id, "lang": "zh-Hant", "variant_key": "alternative"}]
        if action == "pin"
        else []
    )
    selected = controls(
        TargetRecord(
            kind="template_translation",
            data=TemplateTarget(template_id=frame.id, lang="zh-Hant", target=target),
        ),
        named,
        override(binding, action, pins),
    )
    default = {(frame.id, "zh-Hant"): SelectedTarget(target)}
    renderer = Renderer({}, {}, {})
    result = selected.select(field, renderer, default, "zh-Hant")
    if action == "pin":
        value = result.targets[frame.id, "zh-Hant"]
        assert value.target == named.data.target
        assert value.origin == "machine"
        assert value.low_confidence
    else:
        assert result.targets == default
    assert result.suppress == (action == "suppress")
    if action == "suppress":
        with db.transaction():
            db.delete("translation_use", {"id": "use:synthetic"})
            rendered = render_field(
                db, field, result.renderer, result.targets, "zh-Hant", suppress=True
            )
        assert rendered.rendered is None
        assert rendered.issues == ("suppressed_translation",)
        assert read_binding(db, binding.id, {}) == binding
        assert db.rows("translation_selection") == ()
    assert (
        controls(named).select(field, renderer, default, "zh-Hant").targets == default
    )


def test_template_pin_cannot_select_a_different_source_frame(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    pin = override(
        binding,
        "pin",
        [
            {
                "template_id": "frame:" + "f" * 64,
                "lang": "zh-Hant",
                "variant_key": "default",
            }
        ],
    )
    with pytest.raises(ValueError, match="outside the exact source"):
        controls(pin).select(
            _compiled(db, frame, binding), Renderer({}, {}, {}), {}, "zh-Hant"
        )
