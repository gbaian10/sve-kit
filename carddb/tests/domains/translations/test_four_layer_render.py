"""Typed rendering preserves source occurrences, quality and exact concept ranges."""

from dataclasses import replace

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.four_layer import (
    FormDefinition,
    GlossaryReference,
    Target,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.contracts.source_binding import (
    QuantityDomain,
    SourceBinding,
    ZoneDomain,
)
from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Form,
    Label,
    Renderer,
    SelectedTarget,
    validate_form,
)

from ...contracts.test_four_layer import CASES, binding_sample, frame


def sample(target: Target) -> BoundTarget:
    fixture = object_value(object_value(CASES["fixtures"])["identity_base"])
    definition = frame(fixture)
    raw = "TEST_SELECT({zone},{kind},{quantity})"
    checksum = digest(raw.encode())[7:]
    source: dict[str, JsonValue] = {
        "owner": {"kind": "face_revision", "revision_id": "revision:sample"},
        "field": "effect",
        "ordinal": None,
        "source_unit_id": "t:ja:" + checksum[:16],
        "source_hash": checksum,
        "source_ref": {
            "batch_id": "batch:sample",
            "source_version_id": "version:sample",
            "parser": "fixture-parser-v1",
            "locator": "/faces/0/effect",
            "text_hash": checksum,
        },
    }
    spans: dict[str, JsonValue] = {
        "role": "body",
        "segments": [{"start": 0, "end": len(raw)}],
        "anchor": None,
    }
    occurrences: list[JsonValue] = [
        {
            "slot": slot.name,
            "ordinal": 0,
            "raw_spans": [s.model_dump(mode="json") for s in slot.occurrences],
            "canonical_spans": [s.model_dump(mode="json") for s in slot.occurrences],
            "source_unit": None,
            "source_presence": "explicit",
            "resolution_rule": None,
        }
        for slot in definition.leaf_schema.slots
    ]
    trace: list[JsonValue] = [
        {
            "raw_span": {"start": 0, "end": len(raw)},
            "canonical_spans": [{"start": 0, "end": len(raw)}],
            "rule": "identity",
        }
    ]
    data: dict[str, JsonValue] = {
        "source": source,
        "ordinal": 0,
        "line_ordinal": 0,
        "frame_id": definition.id,
        "source_span": spans,
        "values": fixture["values"],
        "occurrences": occurrences,
        "trace": trace,
    }
    payload = {
        "recipe": "binding-v2",
        "occurrence": {
            "owner": source["owner"],
            "field": "effect",
            "ordinal": None,
            "source_hash": checksum,
            "line_ordinal": 0,
            "role": "body",
            "segments": spans["segments"],
        },
        **{
            k: data[k]
            for k in ("frame_id", "source_span", "values", "occurrences", "trace")
        },
    }
    binding = SourceBinding.model_validate_json(
        canonical({"id": "bind:" + hash_payload(payload), **data})
    )
    return BoundTarget(definition, binding, SelectedTarget(target))


def forms() -> dict[tuple[str, str], Form]:
    locative = FormDefinition.model_validate_json(
        canonical(
            {
                "id": "zone.locative",
                "lang": "zh-Hant",
                "rule": "zone.case.v1",
                "signature": [
                    {"name": "zone", "type": "ZoneSet", "role": "counted_zone"}
                ],
                "cases": {
                    zone: [
                        {"kind": "Label", "arg": "zone"},
                        {
                            "kind": "Literal",
                            "text": "上" if zone == "battlefield" else "中",
                        },
                    ]
                    for zone in (
                        "battlefield",
                        "deck",
                        "evolve_deck",
                        "ex",
                        "graveyard",
                        "hand",
                        "mixed",
                    )
                },
            }
        )
    )
    classifier = FormDefinition.model_validate_json(
        canonical(
            {
                "id": "quantity.classifier",
                "lang": "zh-Hant",
                "rule": "quantity.classifier.v1",
                "signature": [
                    {"name": "kind", "type": "CardKind", "role": "counted_kind"},
                    {
                        "name": "quantity",
                        "type": "QuantitySpec",
                        "role": "selection_count",
                    },
                    {"name": "zone", "type": "ZoneSet", "role": "counted_zone"},
                ],
                "cases": {
                    "card": [{"kind": "Literal", "text": "張"}],
                    "object": [{"kind": "Literal", "text": "個"}],
                },
            }
        )
    )
    return {(f.id, f.lang): Form(f) for f in (locative, classifier)}


def renderer(*, bold: bool = True, low: bool = False) -> Renderer:
    hand = GlossaryReference(kind="glossary", key="term:hand")
    follower = VocabularyReference(kind="vocabulary", key=("type", "follower"))
    labels = {
        (canonical(r.model_dump(mode="json")), "zh-Hant"): Label(
            r,
            "zh-Hant",
            text,
            "project",
            low,
            bold if isinstance(r, GlossaryReference) else True,
        )
        for r, text in ((hand, "手牌"), (follower, "從者"))
    }
    return Renderer(
        labels,
        {("ZoneSet", "hand"): hand},
        forms(),
        domains={
            "fixture.zones": ZoneDomain(
                type="ZoneSet", zones=("hand", "battlefield", "ex")
            ),
            "fixture.cardinality": QuantityDomain(
                type="QuantitySpec", modes=("exact",), imports=(), expressions=()
            ),
        },
    )


def target(nodes: list[JsonValue]) -> Target:
    return Target.model_validate_json(canonical({"format": 1, "nodes": nodes}))


def expanded_target() -> Target:
    return target(
        [
            {
                "kind": "Form",
                "form_id": "zone.locative",
                "args": {"zone": {"slot": "zone"}},
            },
            {"kind": "Literal", "text": "的"},
            {"kind": "LeafRef", "slot": "quantity"},
            {
                "kind": "Form",
                "form_id": "quantity.classifier",
                "args": {
                    "kind": {"slot": "kind"},
                    "quantity": {"slot": "quantity"},
                    "zone": {"slot": "zone"},
                },
            },
            {"kind": "LeafRef", "slot": "kind"},
        ]
    )


def test_forms_keep_only_base_labels_in_semantic_ranges() -> None:
    item = sample(expanded_target())
    result = renderer().render("context:sample", "zh-Hant", (item,))
    assert result.rendered is not None
    rendered = result.rendered
    assert rendered.text == "手牌中的2張從者"
    assert [
        rendered.text[r.start : r.end]
        for a in rendered.annotation.occurrences
        for r in a.ranges
    ] == ["手牌", "從者"]
    assert [o.node_path for o in rendered.occurrences()] == [(0, 0), (2,), (4,)]
    assert [o.source_ordinals for o in rendered.occurrences()] == [(0,), (0,), (0,)]


def test_quantity_value_preserves_literal_modifier_and_numeric_occurrence() -> None:
    constant_form = FormDefinition.model_validate_json(
        canonical(
            {
                "id": "quantity.selection_count.value",
                "lang": "zh-Hant",
                "rule": "quantity.constant_value.v1",
                "signature": [
                    {
                        "name": "quantity",
                        "type": "QuantitySpec",
                        "role": "selection_count",
                    }
                ],
                "cases": {"default": [{"kind": "QuantityValue", "arg": "quantity"}]},
            }
        )
    )
    target_value = target(
        [
            {"kind": "LeafRef", "slot": "zone"},
            {"kind": "Literal", "text": "的至多"},
            {
                "kind": "Form",
                "form_id": constant_form.id,
                "args": {"quantity": {"slot": "quantity"}},
            },
            {"kind": "LeafRef", "slot": "kind"},
        ]
    )
    item = sample(target_value)
    data = item.binding.model_dump(mode="json", by_alias=True)
    data["values"]["quantity"]["mode"] = "up_to"
    candidate = SourceBinding.model_validate_json(canonical(data))
    data["id"] = "bind:" + hash_payload(candidate.payload())
    item = replace(item, binding=SourceBinding.model_validate_json(canonical(data)))
    engine = renderer()
    engine.forms[constant_form.id, "zh-Hant"] = Form(constant_form)
    engine.domains["fixture.cardinality"] = QuantityDomain(
        type="QuantitySpec",
        modes=("exact", "up_to", "all"),
        imports=("received",),
        expressions=(),
    )
    result = engine.render("context:quantity", "zh-Hant", (item,)).rendered
    assert result is not None
    assert result.text == "手牌的至多2從者"
    occurrence = result.occurrences()[1]
    assert occurrence.node_path == (2, 0)
    assert occurrence.source_ordinals == (0,)
    assert [(span.start, span.end) for span in occurrence.ranges] == [(5, 6)]
    assert len(result.annotation.occurrences) == 2
    data["values"]["quantity"] = {"mode": "all", "expr": None}
    candidate = SourceBinding.model_validate_json(canonical(data))
    data["id"] = "bind:" + hash_payload(candidate.payload())
    invalid = replace(item, binding=SourceBinding.model_validate_json(canonical(data)))
    with pytest.raises(TypeError, match="QuantityValue requires a constant"):
        engine.render("context:quantity", "zh-Hant", (invalid,))


def test_quantity_value_rule_rejects_label_or_wrong_signature() -> None:
    data: dict[str, JsonValue] = {
        "id": "quantity.selection_count.value",
        "lang": "zh-Hant",
        "rule": "quantity.constant_value.v1",
        "signature": [
            {"name": "quantity", "type": "QuantitySpec", "role": "selection_count"}
        ],
        "cases": {"default": [{"kind": "Label", "arg": "quantity"}]},
    }
    with pytest.raises(ValueError, match="exact role and value part"):
        validate_form(FormDefinition.model_validate_json(canonical(data)))
    data["cases"] = {"default": [{"kind": "QuantityValue", "arg": "quantity"}]}
    data["signature"] = [{"name": "quantity", "type": "Nat", "role": "selection_count"}]
    with pytest.raises(ValueError, match="constant quantity rule"):
        FormDefinition.model_validate_json(canonical(data))


def test_np_grouping_preserves_frame_and_binding_but_changes_presentation_dependency() -> (
    None
):
    expanded = sample(expanded_target())
    grouped = replace(
        expanded,
        selected=SelectedTarget(
            target(
                [
                    {
                        "kind": "NP",
                        "constructor": "CardNP",
                        "args": {
                            "kind": {"slot": "kind"},
                            "quantity": {"slot": "quantity"},
                            "zone": {"slot": "zone"},
                        },
                    }
                ]
            )
        ),
    )
    first = renderer().render("context:sample", "zh-Hant", (expanded,)).rendered
    second = renderer().render("context:sample", "zh-Hant", (grouped,)).rendered
    assert first is not None
    assert second is not None
    assert second.text == "手牌的2張從者"
    assert grouped.frame.id == expanded.frame.id
    assert grouped.binding.id == expanded.binding.id
    assert first.dependency_key != second.dependency_key


def test_repeated_leaf_paths_survive_and_bold_changes_identity_without_changing_text() -> (
    None
):
    item = sample(
        target(
            [
                {"kind": "LeafRef", "slot": "kind"},
                {"kind": "Literal", "text": "😀"},
                {"kind": "LeafRef", "slot": "kind"},
                {"kind": "LeafRef", "slot": "zone"},
                {"kind": "LeafRef", "slot": "quantity"},
            ]
        )
    )
    first = renderer().render("context:sample", "zh-Hant", (item,)).rendered
    second = (
        renderer(bold=False, low=True)
        .render("context:sample", "zh-Hant", (item,))
        .rendered
    )
    assert first is not None
    assert second is not None
    assert first.text == second.text == "從者😀從者手牌2"
    assert first.annotation.text_unit_id == second.annotation.text_unit_id
    assert first.annotation.id != second.annotation.id
    assert first.identity() != second.identity()
    assert second.low_confidence is True
    assert [o.node_path for o in first.occurrences()][:2] == [(0,), (2,)]
    assert [(a.ranges[0].start, a.ranges[0].end) for a in first.annotation.occurrences][
        :2
    ] == [(0, 2), (3, 5)]


def test_missing_label_or_target_returns_whole_field_and_invalid_target_rejects() -> (
    None
):
    item = sample(expanded_target())
    unavailable = Renderer(
        {}, renderer().code_references, forms(), domains=renderer().domains
    )
    result = unavailable.render("context:sample", "zh-Hant", (item,))
    assert result.rendered is None
    assert result.issues == ("missing_term_translation",)
    result = renderer().render(
        "context:sample", "zh-Hant", (replace(item, selected=None),)
    )
    assert result.rendered is None
    assert result.issues == ("missing_template_translation",)
    invalid = replace(
        item, selected=SelectedTarget(target([{"kind": "Literal", "text": "已偷換"}]))
    )
    with pytest.raises(ValueError, match="every required leaf"):
        renderer().render("context:sample", "zh-Hant", (invalid,))


def test_interface_description_does_not_change_render_identity() -> None:
    definition, binding, _, _ = binding_sample()
    item = BoundTarget(
        definition, binding, SelectedTarget(target([{"kind": "LeafRef", "slot": "n"}]))
    )
    first = renderer().render("context:n", "zh-Hant", (item,)).rendered
    data = definition.model_dump(mode="json")
    data["projection"]["scopes"] = [
        {"id": "ability", "parent": None, "kind": "ability"}
    ]
    updated = type(definition).model_validate_json(canonical(data))
    second = (
        renderer()
        .render("context:n", "zh-Hant", (replace(item, frame=updated),))
        .rendered
    )
    assert first is not None
    assert second is not None
    assert first.identity() == second.identity()
    assert definition.projection.interface_key(
        definition.id
    ) != updated.projection.interface_key(updated.id)


def test_form_rule_cases_are_closed_and_complete() -> None:
    original = forms()["zone.locative", "zh-Hant"].definition.model_dump(mode="json")
    for changed in (
        original | {"rule": "run.script"},
        original | {"cases": {"hand": original["cases"]["hand"]}},
    ):
        with pytest.raises(
            ValueError, match=r"Unknown four-layer|exactly its registered"
        ):
            validate_form(FormDefinition.model_validate_json(canonical(changed)))
