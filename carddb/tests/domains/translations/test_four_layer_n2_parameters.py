"""The public preparation entry confines N2 parameters to their source roles."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    FormDefinition,
    FormNode,
    GlossaryReference,
    SlotRef,
    Span,
    Target,
    VocabularyReference,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.catalog.models import Term as VocabularyTerm
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.vocabulary import Binding, Vocabulary
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import (
    ExpressionTargets,
    PreparationRenderer,
)
from sve_carddb.domains.translations.four_layer_preparation import derive_preparation
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Form,
    Label,
    Renderer,
    SelectedTarget,
)

from .test_four_layer_classification import classifier, source

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier


def engine() -> Classifier:
    members = (
        ("type", "crest", "クレストトークン"),
        ("type", "equipment", "イクイップメントトークン"),
        ("type", "follower", "フォロワー"),
        ("class", "synthetic", "仮クラス"),
    )
    return classifier(
        (
            Term("term:ability.activation", "ability", "起動"),
            Term("term:ability.earth_rite", "ability", "土の秘術"),
            Term("term:object.die", "rule_term", "サイコロ"),
            Term("term:stat.attack", "rule_term", "攻撃力"),
            Term("term:stat.health", "rule_term", "体力"),
            Term("term:name.synthetic", "card_name", "仮の道具"),
            Term("term:trait.synthetic", "trait", "仮族"),
        ),
        (
            "bracket_keyword_reference",
            "braced_stat_reference",
            "braced_ability_reference",
        ),
        vocabulary=Vocabulary(
            bindings=tuple(
                Binding(region="jp", kind=kind, code=code, raw=raw)
                for kind, code, raw in members
            ),
            terms=tuple(
                VocabularyTerm(
                    kind=kind,
                    code=code,
                    active=True,
                    label=LocalizedText(lang="ja", text=raw),
                )
                for kind, code, raw in members
            ),
        ),
    )


@pytest.mark.parametrize(("suffix", "amount"), [("", 1), ("_４", 4)])
def test_payment_is_positive_and_omission_has_no_fabricated_span(
    suffix: str, amount: int
) -> None:
    raw = f"【土の秘術{suffix}】これは仮の効果を行う。"
    catalog = engine()
    before = derive_source(raw, source(raw), catalog)
    prepared = derive_preparation(raw, source(raw), catalog)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, catalog, frame, binding)
    slots = {s.role: s for s in frame.leaf_schema.slots}
    slot = slots["earth_rite_amount"]
    assert binding.values[slot.name] == amount
    assert (slot.domain.min, slot.domain.max) == (1, 9007199254740991)
    occurrence = next(o for o in binding.occurrences if o.slot == slot.name)
    assert occurrence.source_presence == ("explicit" if suffix else "omitted")
    assert bool(occurrence.raw_spans) == bool(suffix)
    assert occurrence.resolution_rule == (
        None if suffix else "earth_rite.omitted_amount.v1"
    )
    assert "earth_rite_amount" not in {
        s.role for s in before.recognized[0].schema.slots
    }
    with pytest.raises(ValueError, match=r"domain|construction"):
        prepared.verify(
            raw,
            catalog,
            frame,
            binding.model_copy(update={"values": binding.values | {slot.name: 0}}),
        )


def test_filter_in_payment_line_is_not_another_payment() -> None:
    raw = "【土の秘術】自分の手札の【土の秘術】を持つカード２枚を選ぶ。"
    prepared = derive_preparation(raw, source(raw), engine())
    assert [s.role for s in prepared.derived.recognized[0].schema.slots].count(
        "earth_rite_amount"
    ) == 1


@pytest.mark.parametrize(
    ("condition", "roles", "values"),
    [
        ("２なら、", ["dice_face"], [2]),
        ("２か５なら、", ["dice_face", "dice_face"], [2, 5]),
        ("２から５なら、", ["dice_lower_bound", "dice_upper_bound"], [2, 5]),
    ],
)
def test_die_faces_keep_disjunction_and_interval_distinct(
    condition: str, roles: list[str], values: list[int]
) -> None:
    raw = "サイコロをふる。" + condition + "仮の効果を行う。"
    catalog = engine()
    prepared = derive_preparation(raw, source(raw), catalog)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, catalog, frame, binding)
    slots = [s for s in frame.leaf_schema.slots if s.role.startswith("dice_")]
    assert [s.role for s in slots] == roles
    assert [binding.values[s.name] for s in slots] == values
    assert all(s.domain.min == 1 and s.domain.max == 6 for s in slots)


@pytest.mark.parametrize(
    "raw",
    [
        "【土の秘術】を持つカードを探す。",
        "【土の秘術】を支払ったなら、仮の効果を行う。",
        "このターンに自分の【土の秘術】によって効果を使ったなら、仮の効果を行う。",
        "【土の秘術_０】仮の効果を行う。",
        "サイコロをふる。７なら、仮の効果を行う。",
        "サイコロをふる。５から２なら、仮の効果を行う。",
        "以前出た目が２なら、仮の効果を行う。",
        "クレストが２つなら、仮の効果を行う。",
        "このターン、これを{攻撃力}７/{体力}８のフォロワーにする。",
    ],
)
def test_neighboring_unsealed_sources_keep_complete_v2_evidence(raw: str) -> None:
    catalog = engine()
    before = derive_source(raw, source(raw), catalog)
    assert derive_preparation(raw, source(raw), catalog).derived == before


@pytest.mark.parametrize(
    ("kind", "code"), [("クレスト", "crest"), ("イクイップメント", "equipment")]
)
def test_only_complete_bare_headers_gain_declared_kind(kind: str, code: str) -> None:
    raw = f"『仮の道具』{{仮クラス}}仮族・{kind}" + (
        "{コスト４}" if code == "equipment" else ""
    )
    prepared = derive_preparation(raw, source(raw, "section"), engine())
    ordinal = next(
        p.ordinal
        for p in prepared.derived.field.parts
        if p.source_span.role == "token_header"
    )
    frame, binding = prepared.bind(ordinal)
    prepared.verify(raw, engine(), frame, binding)
    found = next(
        r
        for p, r in zip(
            prepared.derived.field.parts, prepared.derived.recognized, strict=True
        )
        if p.source_span.role == "token_header"
    )
    slots = [s for s in found.schema.slots if s.role == "declared_kind"]
    assert len(slots) == 1
    value = found.values[slots[0].name]
    assert isinstance(value, VocabularyReference)
    assert value.key == ("type", code)


def test_assignment_stays_in_its_attached_reminder() -> None:
    raw = "仮の効果を行う。（このターン、これを{攻撃力}７/{体力}８のフォロワーにする）"
    catalog = engine()
    prepared = derive_preparation(raw, source(raw), catalog)
    index = next(
        p.ordinal
        for p in prepared.derived.field.parts
        if p.source_span.role == "reminder"
    )
    frame, binding = prepared.bind(index)
    prepared.verify(raw, catalog, frame, binding)
    assert binding.source_span.anchor == 0
    assert [
        binding.values[s.name]
        for s in frame.leaf_schema.slots
        if s.role == "stat_value"
    ] == [7, 8]


@pytest.mark.parametrize("suffix", ["", "_４"])
def test_payment_form_uses_normal_labels_ranges_and_missing_label_fallback(
    suffix: str,
) -> None:

    raw = f"【土の秘術{suffix}】これは仮の効果を行う。"
    catalog = engine()
    prepared = derive_preparation(raw, source(raw), catalog)
    frame, binding = prepared.bind(0)
    slots = {s.role: s.name for s in frame.leaf_schema.slots}
    form = FormDefinition.model_validate_json(
        canonical(
            {
                "id": "earth_rite.display",
                "lang": "zh-Hant",
                "rule": "earth_rite.display.v1",
                "signature": [
                    {"name": "ability", "type": "Concept", "role": "ability"},
                    {"name": "amount", "type": "Nat", "role": "earth_rite_amount"},
                ],
                "cases": {
                    "default": [
                        {"kind": "Literal", "text": "【"},
                        {"kind": "Label", "arg": "ability"},
                        {"kind": "Literal", "text": " "},
                        {"kind": "Label", "arg": "amount"},
                        {"kind": "Literal", "text": "】"},
                    ]
                },
            }
        )
    )
    forms = {(form.id, form.lang): Form(form)}
    target = Target(
        format=1,
        nodes=(
            FormNode(
                kind="Form",
                form_id=form.id,
                args={
                    "ability": SlotRef(slot=slots["ability"]),
                    "amount": SlotRef(slot=slots["earth_rite_amount"]),
                },
            ),
        ),
    )
    prepared.verify(raw, catalog, frame, binding, target=target)
    item = BoundTarget(frame, binding, SelectedTarget(target))
    reference = GlossaryReference(kind="glossary", key="term:ability.earth_rite")
    labels = {
        (canonical(reference.model_dump(mode="json")), "zh-Hant"): Label(
            reference, "zh-Hant", "土之秘術", "project", False, True
        )
    }
    renderer = PreparationRenderer(
        Renderer(labels, {}, {}, domains=catalog.domains),
        ExpressionTargets({}),
        preparation_forms=forms,
    )
    rendered = renderer.render("ctx:synthetic", "zh-Hant", (item,))
    assert rendered.rendered is not None
    assert rendered.rendered.text == ("【土之秘術 4】" if suffix else "【土之秘術 1】")
    assert rendered.rendered.annotation.occurrences[0].ranges == (Span(start=1, end=5),)
    missing = PreparationRenderer(
        Renderer({}, {}, {}, domains=catalog.domains),
        ExpressionTargets({}),
        preparation_forms=forms,
    )
    assert missing.render("ctx:synthetic", "zh-Hant", (item,)).issues == (
        "missing_term_translation",
    )
    with pytest.raises(ValueError, match="Unknown"):
        Renderer(labels, {}, forms)


def test_interval_form_renders_only_bounds_and_rejects_hidden_arguments() -> None:
    raw = "サイコロをふる。２から５なら、仮の効果を行う。"
    catalog = engine()
    prepared = derive_preparation(raw, source(raw), catalog)
    frame, binding = prepared.bind(0)
    slots = {s.role: s.name for s in frame.leaf_schema.slots}
    form = FormDefinition.model_validate_json(
        canonical(
            {
                "id": "dice.interval",
                "lang": "zh-Hant",
                "rule": "dice.interval.v1",
                "signature": [
                    {"name": "lower", "type": "Nat", "role": "dice_lower_bound"},
                    {"name": "upper", "type": "Nat", "role": "dice_upper_bound"},
                ],
                "cases": {
                    "default": [
                        {"kind": "Label", "arg": "lower"},
                        {"kind": "Literal", "text": "至"},
                        {"kind": "Label", "arg": "upper"},
                    ]
                },
            }
        )
    )
    reference = GlossaryReference(kind="glossary", key="term:object.die")
    renderer = PreparationRenderer(
        Renderer(
            {
                (canonical(reference.model_dump(mode="json")), "zh-Hant"): Label(
                    reference, "zh-Hant", "骰子", "project", False, False
                )
            },
            {},
            {},
            domains=catalog.domains,
        ),
        ExpressionTargets({}),
        preparation_forms={(form.id, form.lang): Form(form)},
    )
    target = Target.model_validate_json(
        canonical(
            {
                "format": 1,
                "nodes": [
                    {"kind": "LeafRef", "slot": slots["rule_term"]},
                    {
                        "kind": "Form",
                        "form_id": form.id,
                        "args": {
                            "lower": {"slot": slots["dice_lower_bound"]},
                            "upper": {"slot": slots["dice_upper_bound"]},
                        },
                    },
                ],
            }
        )
    )
    prepared.verify(raw, catalog, frame, binding, target=target)
    result = renderer.render(
        "ctx:synthetic",
        "zh-Hant",
        (BoundTarget(frame, binding, SelectedTarget(target)),),
    )
    assert result.rendered is not None
    assert result.rendered.text == "骰子2至5"
    assert len(result.rendered.annotation.occurrences) == 1
    with pytest.raises(ValueError, match="each typed argument"):
        PreparationRenderer(
            Renderer({}, {}, {}),
            ExpressionTargets({}),
            preparation_forms={
                (form.id, form.lang): Form(
                    form.model_copy(update={"cases": {"default": ()}})
                )
            },
        )


def test_die_parameters_preserve_the_printed_position_of_an_existing_omitted_owner() -> (
    None
):
    raw = "{起動}手札２枚を捨てる：サイコロをふる。２なら、仮の効果を行う。"
    catalog = engine()
    before = derive_source(raw, source(raw), catalog).recognized[0]
    prepared = derive_preparation(raw, source(raw), catalog)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, catalog, frame, binding)
    old_roles = {s.name: s.role for s in before.schema.slots}
    new_roles = {s.name: s.role for s in frame.leaf_schema.slots}
    assert any(o.source_presence == "omitted" for o in before.occurrences)
    assert [
        (new_roles[o.slot], o.raw_spans, o.resolution_rule)
        for o in binding.occurrences
        if new_roles[o.slot] in old_roles.values()
    ] == [
        (old_roles[o.slot], o.raw_spans, o.resolution_rule) for o in before.occurrences
    ]
