"""Shared dependency and occurrence vectors exercise actual typed model boundaries."""

from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import GlossaryReference, Target, hash_payload
from sve_carddb.contracts.source_binding import ReferenceDomain, SourceBinding
from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Form,
    Renderer,
    SelectedTarget,
)

from ...contracts.test_four_layer import CASES, annotation_set, changed, frame
from .test_four_layer_render import expanded_target, renderer, sample, target

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.domains.translations.four_layer_render import Rendered

OPERATIONS = {
    "target_references",
    "occurrences",
    "render_dependencies",
    "semantic_dependencies",
}
FIXED = tuple(
    object_value(row)
    for row in array(CASES["cases"])
    if object_value(row)["operation"] in OPERATIONS
)


def rebind(
    item: BoundTarget, definition: Frame, *, values: JsonValue | None = None
) -> BoundTarget:
    data = item.binding.model_dump(mode="json")
    data["frame_id"] = definition.id
    if values is not None:
        data["values"] = values
    binding = SourceBinding.model_validate_json(canonical(data))
    binding = binding.model_copy(
        update={"id": "bind:" + hash_payload(binding.payload())}
    )
    return replace(item, frame=definition, binding=binding)


def rendered(item: BoundTarget, selected: Renderer | None = None) -> Rendered:
    result = (selected or renderer()).render("context:fixed", "zh-Hant", (item,))
    assert result.rendered is not None
    return result.rendered


def _target(value: dict[str, JsonValue]) -> dict[str, JsonValue]:
    base = object_value(object_value(CASES["fixtures"])[string(value["fixture"])])
    updated = changed(base, object_value(value.get("changes", {})))
    item = sample(Target.model_validate_json(canonical(updated["target"])))
    bound = set(map(string, array(updated["bound"])))
    item = rebind(
        item,
        item.frame,
        values={
            key: val
            for key, val in object_value(
                item.binding.model_dump(mode="json")["values"]
            ).items()
            if key in bound
        },
    )
    if "reference" in value:
        definition = deepcopy(
            object_value(object_value(CASES["fixtures"])["identity_base"])
        )
        slots = array(
            object_value(object_value(definition["semantic"])["leaf_schema"])["slots"]
        )
        object_value(slots[0])["type"] = "Concept"
        object_value(slots[0])["domain"] = {
            "values": ["concept.rule_term.v1"],
            "min": None,
            "max": None,
        }
        values = object_value(item.binding.model_dump(mode="json")["values"])
        values[string(object_value(slots[0])["name"])] = value["reference"]
        item = rebind(item, frame(definition), values=values)
        domains = dict(renderer().domains)
        domains["concept.rule_term.v1"] = ReferenceDomain(
            type="Concept", category="rule_term", references=()
        )
        item.binding.verify(item.frame, domains)
    rendered(item)
    return {}


def _present(value: dict[str, JsonValue]) -> dict[str, JsonValue]:
    item = sample(expanded_target())
    before = rendered(item)
    selected = renderer()
    after_item = item
    key = (
        canonical(
            GlossaryReference(kind="glossary", key="term:hand").model_dump(mode="json")
        ),
        "zh-Hant",
    )
    action = string(value["changed"])
    if action in {"label", "bold", "low_confidence", "emphasis_to_null"}:
        label = selected.labels[key]
        if action == "label":
            label = replace(label, text="掌中牌")
        elif action == "low_confidence":
            label = replace(label, low_confidence=True)
        else:
            label = replace(label, bold=None if action == "emphasis_to_null" else False)
        selected = Renderer(
            selected.labels | {key: label},
            selected.code_references,
            selected.forms,
            domains=selected.domains,
        )
    elif action == "form":
        form_key = ("zone.locative", "zh-Hant")
        original = selected.forms[form_key]
        data = original.definition.model_dump(mode="json")
        object_value(data["cases"])["hand"] = [
            {"kind": "Label", "arg": "zone"},
            {"kind": "Literal", "text": "之中"},
        ]
        replacement_form = Form(
            type(original.definition).model_validate_json(canonical(data))
        )
        selected = Renderer(
            selected.labels,
            selected.code_references,
            selected.forms | {form_key: replacement_form},
            domains=selected.domains,
        )
    elif action == "target_np":
        grouped = target(
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
        after_item = replace(item, selected=SelectedTarget(grouped))
    else:
        fixture = deepcopy(
            object_value(object_value(CASES["fixtures"])["identity_base"])
        )
        fixture["note"] = "Edited private note"
        after_item = rebind(item, frame(fixture))
    result = selected.render("context:fixed", "zh-Hant", (after_item,))
    assert result.rendered is not None
    return {
        "frame_id_changed": after_item.frame.id != item.frame.id,
        "render_dependency_changed": before.dependency_key
        != result.rendered.dependency_key,
        "dsl_review_stale": before_semantics(item) != before_semantics(after_item),
        "bold": result.rendered.annotation.occurrences[0].bold,
        "reason": None if not result.issues else result.issues[0],
    }


def before_semantics(item: BoundTarget, body: str = "body:one") -> str:
    """DSL consumers pin source values and interface, never presentation dependencies."""
    return hash_payload(
        {
            "frame": item.frame.id,
            "binding": item.binding.id,
            "interface": item.frame.projection.interface_key(item.frame.id),
            "body": body,
        }
    )


def _semantic(value: dict[str, JsonValue]) -> dict[str, JsonValue]:
    original = object_value(object_value(CASES["fixtures"])["identity_base"]) | {
        "card_id": "card:fixed"
    }
    fixture = deepcopy(original)
    first = sample(expanded_target())
    action = string(value["changed"])
    body = "body:one"
    if action == "semantic_variant":
        object_value(object_value(fixture["semantic"])["semantic_variant"])["key"] = (
            "activated_choose_effect"
        )
    elif action == "projection.projection_kind":
        object_value(object_value(fixture["semantic"])["projection"])[
            "projection_kind"
        ] = "card_field"
    elif action.startswith("projection."):
        member = action.split(".")[1]
        identity = next(
            object_value(row)
            for row in array(CASES["cases"])
            if object_value(row)["operation"] == "frame_identity"
            and object_value(row)["id"] == "projection_" + member
        )
        fixture = changed(
            fixture, object_value(object_value(identity["input"])["changes"])
        )
    elif action == "dsl_body":
        body = "body:two"
    second = rebind(first, frame(fixture))
    if action == "source_hash":
        data = second.binding.model_dump(mode="json")
        source = object_value(data["source"])
        checksum = digest(b"TEST_SELECT({zone},{kind},{quantity}) ")[7:]
        source["source_hash"] = checksum
        source["source_unit_id"] = "t:ja:" + checksum[:16]
        object_value(source["source_ref"])["text_hash"] = checksum
        binding = SourceBinding.model_validate_json(canonical(data))
        binding = binding.model_copy(
            update={"id": "bind:" + hash_payload(binding.payload())}
        )
        second = replace(second, binding=binding)
    first.binding.verify(first.frame, renderer().domains)
    second.binding.verify(second.frame, renderer().domains)
    return {
        "frame_rekey": first.frame.id != second.frame.id,
        "bindings_stale": first.binding.id != second.binding.id,
        "render_stale": rendered(first).dependency_key
        != rendered(second).dependency_key,
        "dsl_stale": before_semantics(first) != before_semantics(second, body),
        "ruling_uses_recheck": (first.frame.id, first.binding.id)
        != (second.frame.id, second.binding.id),
        "permanent_card_id_changed": original["card_id"] != fixture["card_id"],
        "interface_key_changed": first.frame.projection.interface_key(first.frame.id)
        != second.frame.projection.interface_key(second.frame.id),
    }


def repeated() -> BoundTarget:
    item = sample(
        target(
            [
                {"kind": "LeafRef", "slot": "zone"},
                {"kind": "Literal", "text": "與"},
                {"kind": "LeafRef", "slot": "zone"},
            ]
        )
    )
    fixture = deepcopy(object_value(object_value(CASES["fixtures"])["identity_base"]))
    slots = array(
        object_value(object_value(fixture["semantic"])["leaf_schema"])["slots"]
    )
    zone = object_value(slots[0])
    zone["occurrences"] = array(zone["occurrences"]) + array(
        object_value(slots[1])["occurrences"]
    )
    object_value(object_value(fixture["semantic"])["leaf_schema"])["slots"] = [zone]
    definition = frame(fixture)
    data = item.binding.model_dump(mode="json")
    data["frame_id"] = definition.id
    data["values"] = {"zone": ["hand"]}
    occurrences = array(data["occurrences"])
    duplicate = object_value(occurrences[1]) | {"slot": "zone", "ordinal": 1}
    data["occurrences"] = [occurrences[0], duplicate]
    binding = SourceBinding.model_validate_json(canonical(data))
    binding = binding.model_copy(
        update={"id": "bind:" + hash_payload(binding.payload())}
    )
    return replace(item, frame=definition, binding=binding)


def _occurrences(value: dict[str, JsonValue]) -> dict[str, JsonValue]:
    if "target_paths" in value or "target_occurrences" in value:
        selected = renderer()
        reference = GlossaryReference(
            kind="glossary", key=string(value.get("reference", "term:zone.hand"))
        )
        old = next(
            label
            for label in selected.labels.values()
            if isinstance(label.reference, GlossaryReference)
        )
        label = replace(old, reference=reference)
        labels = {key: item for key, item in selected.labels.items() if item != old}
        labels[canonical(reference.model_dump(mode="json")), "zh-Hant"] = label
        selected = Renderer(
            labels,
            {("ZoneSet", "hand"): reference},
            selected.forms,
            domains=selected.domains,
        )
        result = rendered(repeated(), selected)
        rows = result.occurrences()
        if "render_occurrence_rows" in value:
            result.verify_occurrences(rows[: integer(value["render_occurrence_rows"])])
        else:
            assert result.text == value["text"]
            assert [list(row.node_path) for row in rows] == value["target_paths"]
            assert [list(row.source_ordinals) for row in rows] == [
                value["source_ordinals"]
            ] * len(rows)
            assert [
                span.model_dump(mode="json") for row in rows for span in row.ranges
            ] == value["ranges"]
        return {
            "occurrence_count": len(result.annotation.occurrences),
            "dedup_term_count": len(
                {
                    canonical(a.reference.model_dump(mode="json"))
                    for a in result.annotation.occurrences
                }
            ),
            "render_occurrence_count": len(rows),
        }
    text = string(value.get("annotation_text", value.get("text")))
    ranges = value.get("ranges", [{"start": 0, "end": len(text)}])
    annotation = annotation_set(
        text,
        [
            {
                "ordinal": 0,
                "reference": {"kind": "glossary", "key": "term:zone.hand"},
                "ranges": ranges,
                "bold": True,
            }
        ],
    )
    if "selected_text" in value:
        selected_text = string(value["selected_text"])
        annotation.verify(
            "t:zh-Hant:" + digest(selected_text.encode())[7:23],
            "zh-Hant",
            selected_text,
        )
    elif "translation" in value:
        item = replace(sample(expanded_target()), selected=None)
        unavailable = renderer().render("context:fixed", "zh-Hant", (item,))
        assert unavailable.rendered is None
        annotation.verify(annotation.text_unit_id, "zh-Hant", text)
        return {
            "source_annotation_retained": len(annotation.occurrences)
            == integer(value["source_annotations"])
        }
    return {}


DISPATCH = {
    "target_references": _target,
    "render_dependencies": _present,
    "semantic_dependencies": _semantic,
    "occurrences": _occurrences,
}
REASONS = {
    "missing_required_leaf": "missing required",
    "unused_required_leaf": "every required leaf",
    "unknown_leaf_reference": "Target references an unknown leaf",
    "unknown_concept": "outside every registered source domain",
    "annotation_text_mismatch": "text identity mismatch",
    "span_out_of_bounds": "outside exact text",
    "missing_render_occurrence": "render occurrence",
}


@pytest.mark.parametrize(
    "case",
    FIXED,
    ids=lambda c: (
        string(c["operation"]) + ":" + string(c["group"]) + "/" + string(c["id"])
    ),
)
def test_shared_operation(case: dict[str, JsonValue]) -> None:
    expected = object_value(case["expected"])
    invoke = DISPATCH[string(case["operation"])]
    value = object_value(case["input"])
    if expected["result"] == "reject":
        with pytest.raises(ValueError, match=REASONS[string(expected["reason"])]):
            invoke(value)
    else:
        actual = invoke(value)
        for key, result in expected.items():
            if key != "result":
                assert actual[key] == result
