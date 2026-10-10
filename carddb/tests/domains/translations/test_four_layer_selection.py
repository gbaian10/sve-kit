"""Explicit source selection keeps reusable wording separate from source semantics."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import GlossaryReference, Target
from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.translations.four_layer_authored import (
    ChoiceVariantRecord,
    FrameRecord,
    Inputs,
    MatchRecord,
    OverrideRecord,
    TargetRecord,
    TargetVariantRecord,
    TemplateTarget,
)
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_fields import render_field
from sve_carddb.domains.translations.four_layer_matching import Frames, Matched
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_pipeline import CompiledField
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)
from sve_carddb.domains.translations.four_layer_selection import Controls
from sve_carddb.domains.translations.four_layer_sources import CardSource
from sve_carddb.domains.translations.four_layer_storage import read_binding

from ...support.digital_link_import_fixtures import make_fixture
from ...support.translation_fixtures import choice, reference
from .test_four_layer_classification import classifier, source
from .test_four_layer_fields import compiled_field
from .test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared SQLite schema fixture
from .test_four_layer_storage import default_target
from .test_four_layer_storage import stored as stored  # ruff: ignore[useless-import-alias] -- shared exact source fixture

if TYPE_CHECKING:
    from pathlib import Path

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
    binding: SourceBinding,
    action: str,
    templates: list[JsonValue],
    terms: list[JsonValue] | None = None,
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
                    "terms": terms or [],
                    "reason": "Synthetic wording selection",
                },
            }
        )
    )


def test_manual_match_uses_actual_verified_values_and_rejects_stale_or_partial_selection(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    field = compiled_field(db, frame, binding)
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
    field = compiled_field(db, frame, binding)
    target = default_target()
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
            compiled_field(db, frame, binding), Renderer({}, {}, {}), {}, "zh-Hant"
        )


@pytest.mark.parametrize("variant", ["default", "alternative"])
def test_term_pin_and_named_glossary_wording_use_the_exact_source_reference(  # ruff: ignore[too-many-locals] -- real recognition, wording evidence, selection and rendering jointly exercise the public flow
    stored: tuple[Database, Frame, SourceBinding], tmp_path: Path, variant: str
) -> None:
    db, _, _ = stored
    term = Term("term:stat.attack", "rule_term", "攻撃力")
    raw = "{攻撃力}を仮。"
    normalized = normalize_source(raw, source(raw))
    engine = classifier((term,), ("braced_stat_reference",))
    frame, binding = engine.recognize(raw, normalized.source, normalized.parts[0]).bind(
        normalized.source, normalized.parts[0]
    )
    field = CompiledField(
        CardSource(binding.source, raw, "card", "face", "source"),
        normalized,
        (Matched(frame, binding),),
        (False,),
        (),
    )
    ref = GlossaryReference(kind="glossary", key=term.id)
    label = Label(ref, "zh-Hant", "合成攻擊值", "project", False, True)
    renderer = Renderer(
        {(canonical(ref.model_dump(mode="json")), "zh-Hant"): label},
        {},
        {},
        domains=engine.domains,
    )
    named = ChoiceVariantRecord.model_validate_json(
        canonical(
            choice("stat.attack", value="另一合成攻擊值")
            | {
                "kind": "glossary_choice_variant",
                "data": {
                    **object_value(
                        choice("stat.attack", value="另一合成攻擊值")["data"]
                    ),
                    "variant_key": "alternative",
                },
            }
        )
    )
    with db.transaction():
        db.insert(
            "glossary_term",
            {
                "id": term.id,
                "category": term.category,
                "concept_key": "stat.attack",
                "source_ja": term.source_ja,
                "emphasis": True,
                "authored_source_id": "source",
                "record_key": "synthetic-term",
                "origin": "project",
                "low_confidence": False,
            },
        )
    selections: list[JsonValue] = [
        {"term_id": term.id, "lang": "zh-Hant", "variant_key": variant}
    ]
    selected = controls(named, override(binding, "pin", [], selections))
    selected.prepare_labels(db, engine, make_fixture(tmp_path).sources())
    result = selected.select(field, renderer, {}, "zh-Hant")
    target = Target.model_validate_json(
        canonical({"format": 1, "nodes": [{"kind": "LeafRef", "slot": "leaf_0"}]})
    )
    rendered = result.renderer.render(
        "context:synthetic",
        "zh-Hant",
        (BoundTarget(frame, binding, SelectedTarget(target)),),
    )
    assert rendered.rendered is not None
    assert rendered.rendered.text == (
        "合成攻擊值" if variant == "default" else "另一合成攻擊值"
    )
    assert rendered.rendered.annotation.occurrences[0].reference == ref
    absent = controls(named, override(binding, "pin", [], selections))
    absent.prepare_labels(db, engine, make_fixture(tmp_path / "other").sources())
    assert field.matches[0] is not None
    unrelated = field.matches[0].binding.model_copy(update={"values": {}})
    foreign = field.__class__(
        field.source,
        field.field,
        (Matched(frame, unrelated),),
        field.low_confidence,
        field.issues,
    )
    with pytest.raises(ValueError, match="outside the exact source"):
        absent.select(foreign, renderer, {}, "zh-Hant")
    if variant == "default":
        with pytest.raises(ValueError, match="no usable label"):
            selected.select(
                field, Renderer({}, {}, {}, domains=engine.domains), {}, "zh-Hant"
            )
    else:
        bad = named.model_dump(mode="json", exclude={"record_key"})
        data = bad["data"]
        assert isinstance(data, dict)
        data["value"] = {"kind": "source", "source_ref": reference(), "span": None}
        unavailable = ChoiceVariantRecord.model_validate_json(canonical(bad))
        with pytest.raises(ValueError, match=r"[Bb]atch|[Ss]ource|[Ff]rozen"):
            controls(unavailable).prepare_labels(
                db, engine, make_fixture(tmp_path / "missing").sources()
            )
