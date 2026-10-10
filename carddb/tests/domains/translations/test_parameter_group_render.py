"""Render-only NP vectors and separate regressions for the frozen N0 producer."""

from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import ConfigDict, JsonValue, TypeAdapter

from sve_carddb.contracts.four_layer import (
    FormDefinition,
    LeafType,
    Reference,
    Span,
    Target,
    hash_payload,
)
from sve_carddb.contracts.source_binding import (
    LeafOccurrence,
    QuantityDomain,
    ReferenceDomain,
    SourceBinding,
    SourceSpan,
    TracePiece,
    TypedValue,
    ZoneDomain,
)
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.domains.catalog.models import Term as VocabularyTerm
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.vocabulary import Binding, Vocabulary
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Form,
    Label,
    Renderer,
    SelectedTarget,
    validate_form,
)

from ...contracts.test_four_layer import frame
from .test_four_layer_classification import classifier, source

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_render import Rendered

DATA = object_value(
    TypeAdapter(JsonValue).validate_json(
        (
            Path(__file__).parents[2] / "fixtures/parameter-groups/render-cases.json"
        ).read_bytes()
    )
)
CASES = tuple(object_value(c) for c in array(DATA["cases"]))


def engine() -> Renderer:
    references: TypeAdapter[Reference] = TypeAdapter(
        Reference, config=ConfigDict(regex_engine="python-re")
    )
    labels = tuple(
        (
            references.validate_json(canonical(object_value(label)["reference"])),
            string(object_value(label)["label"]),
        )
        for label in array(DATA["labels"])
    )
    forms = tuple(
        FormDefinition.model_validate_json(canonical(f)) for f in array(DATA["forms"])
    )
    return Renderer(
        {
            (canonical(ref.model_dump(mode="json")), "zh-Hant"): Label(
                ref, "zh-Hant", text, "project", False, True
            )
            for ref, text in labels
        },
        {
            (
                TypeAdapter(LeafType).validate_python(kind),
                code,
            ): references.validate_json(canonical(ref))
            for kind, codes in object_value(DATA["code_labels"]).items()
            for code, ref in object_value(codes).items()
        },
        {(f.id, f.lang): Form(f) for f in forms},
        domains={
            "fixture.pg380.zone": ZoneDomain(
                type="ZoneSet", zones=("battlefield", "hand", "ex", "deck", "cemetery")
            ),
            "fixture.pg380.zone_union": ZoneDomain(
                type="ZoneSet", zones=("battlefield", "ex")
            ),
            "fixture.pg380.quantity": QuantityDomain(
                type="QuantitySpec",
                modes=("exact", "up_to"),
                imports=(),
                expressions=(),
            ),
        },
    )


def render_item(fixture: dict[str, JsonValue]) -> BoundTarget:
    definition = frame(fixture)
    raw = string(object_value(fixture["semantic"])["canonical_source"])
    descriptor = source(raw)
    binding = SourceBinding(
        id="bind:" + "0" * 64,
        source=descriptor,
        ordinal=0,
        line_ordinal=0,
        frame_id=definition.id,
        source_span=SourceSpan(
            role="body", segments=(Span(start=0, end=len(raw)),), anchor=None
        ),
        values=TypeAdapter(
            dict[str, TypedValue], config=ConfigDict(regex_engine="python-re")
        ).validate_json(canonical(fixture["values"])),
        occurrences=tuple(
            LeafOccurrence(
                slot=s.name,
                ordinal=0,
                raw_spans=s.occurrences,
                canonical_spans=s.occurrences,
                source_unit=string(fixture["source_unit"])
                if s.name == "quantity"
                else None,
                source_presence="explicit",
                resolution_rule=None,
            )
            for s in definition.leaf_schema.slots
        ),
        trace=(
            TracePiece(
                raw_span=Span(start=0, end=len(raw)),
                canonical_spans=(Span(start=0, end=len(raw)),),
                rule="identity",
            ),
        ),
    )
    # This is a renderer fixture, never a claim that N0 recognizes its typed leaves.
    binding = SourceBinding.model_validate_json(
        canonical(binding.model_dump(mode="json"))
    )
    binding = binding.model_copy(
        update={"id": "bind:" + hash_payload(binding.payload())}
    )
    return BoundTarget(
        definition,
        binding,
        SelectedTarget(Target.model_validate_json(canonical(fixture["target"]))),
    )


def rendered(item: BoundTarget, selected: Renderer) -> Rendered:
    output = selected.render("context:pg380", "zh-Hant", (item,))
    assert output.rendered is not None
    return output.rendered


@pytest.mark.parametrize("case", CASES, ids=lambda c: string(c["id"]))
def test_parameter_group_case(case: dict[str, JsonValue]) -> None:
    value = object_value(case["input"])
    expected = object_value(case["expected"])
    if case["operation"] == "parameter_group_n0":
        raw = string(value["source"])
        field = normalize_source(raw, source(raw))
        selected = classifier()
        if value["class"]:
            selected.references.vocabulary = Vocabulary(
                bindings=(
                    Binding(region="jp", kind="class", raw="検査クラス", code="probe"),
                ),
                terms=(
                    VocabularyTerm(
                        kind="class",
                        code="probe",
                        label=LocalizedText(lang="ja", text="検査クラス"),
                    ),
                ),
            )
        part = field.parts[0]
        found = selected.recognize(raw, field.source, part, field=field)
        assert not found.issues
        definition, binding = found.bind(field.source, part)
        selected.verify(raw, field, part, definition, binding)
        assert any(
            o.source_unit == expected["selection_unit"] for o in binding.occurrences
        )
        assert not any(
            s.type == "CardKind" and s.role == "counted_kind"
            for s in definition.leaf_schema.slots
        )
        assert expected["np_eligible"] is False
        return
    fixture = object_value(object_value(DATA["fixtures"])[string(value["fixture"])])
    item = render_item(fixture)
    grouped = replace(
        item,
        selected=SelectedTarget(
            Target.model_validate_json(canonical(fixture["np_target"]))
        ),
    )
    first, second = rendered(item, engine()), rendered(grouped, engine())
    assert first.text == second.text == expected["text"]
    assert first.annotation == second.annotation
    assert [
        a.model_dump(mode="json") for a in second.annotation.occurrences
    ] == expected["annotations"]
    assert item.frame.id == grouped.frame.id
    assert item.binding == grouped.binding
    assert first.dependency_key != second.dependency_key
    assert [o.source_ordinals for o in first.occurrences()] == [
        o.source_ordinals for o in second.occurrences()
    ]
    assert [o.ranges for o in first.occurrences()] == [
        o.ranges for o in second.occurrences()
    ]


@pytest.mark.parametrize(
    ("key", "form_id", "issue"),
    [
        ("g07", "zone.card_np", "missing_card_zone_form"),
        ("g07", "quantity.classifier", "missing_classifier_form"),
        ("g09", "trait.filter", "missing_trait_filter_form"),
        ("g10", "zone.union_separator", "missing_zone_union_separator"),
    ],
)
def test_card_selection_requires_complete_language_form_and_labels(
    key: str, form_id: str, issue: str
) -> None:
    fixture = object_value(object_value(DATA["fixtures"])[key])
    item = replace(
        render_item(fixture),
        selected=SelectedTarget(
            Target.model_validate_json(canonical(fixture["np_target"]))
        ),
    )
    selected = engine()
    del selected.forms[form_id, "zh-Hant"]
    assert selected.render("context:pg380", "zh-Hant", (item,)).issues == (issue,)
    selected = engine()
    selected.labels.clear()
    assert selected.render("context:pg380", "zh-Hant", (item,)).issues == (
        "missing_term_translation",
    )


def test_form_cannot_hide_a_required_modifier_or_change_its_type() -> None:
    definition = next(
        object_value(f)
        for f in array(DATA["forms"])
        if object_value(f)["id"] == "trait.filter"
    )
    for mutation in ("hide", "type"):
        changed = deepcopy(definition)
        if mutation == "hide":
            cases = object_value(changed["cases"])
            cases["default"] = [
                p
                for p in array(cases["default"])
                if object_value(p).get("arg") != "trait"
            ]
        else:
            object_value(array(changed["signature"])[0])["type"] = "TokenStatus"
        with pytest.raises(ValueError, match="exact typed signature"):
            validate_form(FormDefinition.model_validate_json(canonical(changed)))


def test_multi_trait_relation_is_not_assumed_by_card_np() -> None:
    fixture = deepcopy(object_value(object_value(DATA["fixtures"])["g09"]))
    nodes = array(object_value(fixture["np_target"])["nodes"])
    object_value(object_value(nodes[1])["args"])["traits"] = [
        {"slot": "trait"},
        {"slot": "kind"},
    ]
    item = render_item(fixture)
    item = replace(
        item,
        selected=SelectedTarget(
            Target.model_validate_json(canonical(fixture["np_target"]))
        ),
    )
    with pytest.raises(ValueError, match="at most one"):
        engine().render("context:pg380", "zh-Hant", (item,))


@pytest.mark.parametrize(
    ("key", "name", "domain", "category"),
    [
        ("g09", "trait", "concept.trait.v1", "trait"),
        ("g09_class", "class", "vocabulary.class.v1", "class"),
    ],
)
def test_registered_reference_domains_preserve_np_annotations(
    key: str, name: str, domain: str, category: str
) -> None:
    fixture = deepcopy(object_value(object_value(DATA["fixtures"])[key]))
    slots = array(
        object_value(object_value(fixture["semantic"])["leaf_schema"])["slots"]
    )
    slot = next(object_value(s) for s in slots if object_value(s)["name"] == name)
    slot["domain"] = {"values": [domain], "min": None, "max": None}
    selected = engine()
    selected.domains[domain] = ReferenceDomain.model_validate_json(
        canonical(
            {
                "type": "Concept",
                "category": category,
                "references": [object_value(fixture["values"])[name]],
            }
        )
    )
    item = render_item(fixture)
    grouped = replace(
        item,
        selected=SelectedTarget(
            Target.model_validate_json(canonical(fixture["np_target"]))
        ),
    )
    assert rendered(item, selected).annotation == rendered(grouped, selected).annotation


@pytest.mark.parametrize(("key", "name"), [("g09_class", "class"), ("g09", "token")])
def test_class_or_token_cannot_be_a_trait(key: str, name: str) -> None:
    fixture = deepcopy(object_value(object_value(DATA["fixtures"])[key]))
    nodes = array(object_value(fixture["np_target"])["nodes"])
    args = object_value(object_value(nodes[1])["args"])
    args.pop(name)
    args["traits"] = [{"slot": name}]
    item = replace(
        render_item(fixture),
        selected=SelectedTarget(
            Target.model_validate_json(canonical(fixture["np_target"]))
        ),
    )
    with pytest.raises(ValueError, match=r"glossary concepts|leaf type"):
        engine().render("context:pg380", "zh-Hant", (item,))


def test_card_np_cannot_use_a_destination_as_its_counted_zone() -> None:
    fixture = deepcopy(object_value(object_value(DATA["fixtures"])["g07"]))
    slots = array(
        object_value(object_value(fixture["semantic"])["leaf_schema"])["slots"]
    )
    next(object_value(s) for s in slots if object_value(s)["name"] == "zone")[
        "role"
    ] = "destination_zone"
    item = replace(
        render_item(fixture),
        selected=SelectedTarget(
            Target.model_validate_json(canonical(fixture["np_target"]))
        ),
    )
    with pytest.raises(ValueError, match="counted source zone"):
        engine().render("context:pg380", "zh-Hant", (item,))
