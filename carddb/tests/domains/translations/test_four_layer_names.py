"""Name predicates preserve independent references and reject ambiguous source ports."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    ContextExpression,
    GlossaryReference,
    LeafRef,
    LiteralNode,
    PredicateExpr,
    Target,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_analysis import analyze_source
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import ExpressionTargets
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from .expression_fixtures import expression_plan
from .test_four_layer_classification import source
from .test_four_layer_n1 import n1_classifier


def engine() -> Classifier:
    base = n1_classifier()
    term = Term("term:trait.other", "trait", "仮別族")
    terms = (*base.terms.values(), term)
    refs = replace(
        base.references,
        terms={**base.references.terms, term.source_ja: [(term.id, term.category)]},
    )
    return Classifier(terms, refs, base.rules)


@pytest.mark.parametrize(
    ("raw", "identifier"),
    [
        (
            "自分のデッキから「これと同名でない仮族・カード」7枚を探し、手札に加える。",
            "name.relative.holder.not_equal.v1",
        ),
        (
            "自分の墓場のカードをカード名が異なるように7枚選ぶ。",
            "name.selection.distinct.v1",
        ),
        (
            "自分の墓場にあるスペルのカード名の種類数。",
            "name.distinct_count.objects.v1",
        ),
        (
            "自分の場のフォロワー1体を選ぶ。それと同名のカード7枚をEXエリアに置く。",
            "name.relative.objects.equal.v1",
        ),
        (
            "手札のフォロワー1枚を公開：自分のデッキから「公開したフォロワーと同名のフォロワー」7枚を探す。",
            "name.relative.receipt.equal.v1",
        ),
    ],
)
def test_name_relations_have_typed_source_ports(raw: str, identifier: str) -> None:
    classifier = engine()
    before = analyze_source(raw, source(raw), classifier)
    prepared = derive_source(raw, source(raw), classifier)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, classifier, frame, binding)
    expr = next(
        v
        for v in binding.values.values()
        if isinstance(v, (PredicateExpr, ContextExpression))
    )
    assert expr.expression_id == identifier
    assert bool(isinstance(expr, ContextExpression)) == ("distinct_count" in identifier)
    assert all(p.evidence is not None for p in frame.projection.imports)
    assert prepared.derived.recognized[0].schema != before.recognized[0].schema


def test_ambiguous_or_cross_ability_receipt_remains_pending() -> None:
    for prefix in (
        "",
        "手札のフォロワー1枚を公開：手札のフォロワー1枚を公開：",
        "手札のフォロワー1枚を公開：{起動}",
    ):
        raw = (
            prefix
            + "自分のデッキから「公開したフォロワーと同名のフォロワー」7枚を探す。"
        )
        prepared = derive_source(raw, source(raw), engine())
        assert (
            "name_source_missing_or_ambiguous" in prepared.derived.recognized[0].issues
        )


@pytest.mark.parametrize("translated_fragment", ["測試片段", "完全不同的官方譯名"])
def test_fragments_render_with_required_reference_annotation(  # ruff: ignore[too-many-locals] -- keep source, dependency closure and normal rendering in one case
    translated_fragment: str,
) -> None:
    raw = "自分の墓場の「カード名に『仮場手札自分』を含むフォロワー」7枚を選ぶ。"
    classifier = engine()
    prepared = derive_source(raw, source(raw), classifier)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, classifier, frame, binding)
    fragment = next(s for s in frame.leaf_schema.slots if s.role == "name_fragment")
    predicate = next(
        s for s in frame.leaf_schema.slots if s.role == "name_contains_filter"
    )
    assert fragment.required
    assert binding.values[fragment.name] == CardNameReference(
        kind="card_name", term_id="term:name.synthetic"
    )
    reference = binding.values[fragment.name]
    assert isinstance(reference, CardNameReference)
    base = Renderer({}, {}, {}, domains=classifier.domains)
    labels = {
        (canonical(reference.model_dump(mode="json")), "zh-Hant"): Label(
            reference, "zh-Hant", translated_fragment, "official", True, True
        ),
        (canonical(reference.model_dump(mode="json")), "ja"): Label(
            reference, "ja", "仮場手札自分", "project", False, True
        ),
    }
    for slot in frame.leaf_schema.slots:
        value = binding.values[slot.name]
        if slot.type in {"Player", "ZoneSet"}:
            code = value[0] if isinstance(value, tuple) else value
            assert isinstance(code, str)
            ref = base.code_references[slot.type, code]
            labels[canonical(ref.model_dump(mode="json")), "zh-Hant"] = Label(
                ref, "zh-Hant", code, "project", False, True
            )
    renderer = Renderer(labels, {}, {}, domains=classifier.domains)
    expression = SelectedTarget(
        Target(
            format=1,
            nodes=(
                LiteralNode(kind="Literal", text="名稱含有"),
                LeafRef(kind="LeafRef", slot="fragment"),
            ),
        )
    )
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=tuple(
                LeafRef(kind="LeafRef", slot=s.name)
                for s in frame.leaf_schema.slots
                if s.required and s.name != fragment.name
            ),
        )
    )
    targets = ExpressionTargets(
        {("name.contains.fragment.v1", "zh-Hant"): expression},
        registry=prepared.registry,
    )
    presenter = renderer
    result = presenter.render(
        "ctx:synthetic",
        "zh-Hant",
        expression_plan((BoundTarget(frame, binding, selected),), targets),
    )
    assert result.rendered is not None
    assert "名稱含有仮場手札自分" in result.rendered.text
    assert translated_fragment not in result.rendered.text
    assert result.rendered.low_confidence is False
    assert result.rendered.annotation.occurrences
    occurrence = next(
        a for a in result.rendered.annotation.occurrences if a.reference == reference
    )
    assert occurrence.bold is True
    assert [result.rendered.text[s.start : s.end] for s in occurrence.ranges] == [
        "仮場手札自分"
    ]
    assert any(leaf.slot == fragment.name for leaf in result.rendered.leaves)
    del renderer.labels[canonical(reference.model_dump(mode="json")), "zh-Hant"]
    without_translation = renderer.render(
        "ctx:synthetic",
        "zh-Hant",
        expression_plan((BoundTarget(frame, binding, selected),), targets),
    )
    assert without_translation.rendered is not None
    assert without_translation.rendered.identity() == result.rendered.identity()
    del renderer.labels[canonical(reference.model_dump(mode="json")), "ja"]
    missing = renderer.render(
        "ctx:synthetic",
        "zh-Hant",
        expression_plan((BoundTarget(frame, binding, selected),), targets),
    )
    assert missing.rendered is None
    assert missing.issues == ("missing_term_translation",)
    invalid = Target(
        format=1,
        nodes=tuple(
            n
            for n in selected.target.nodes
            if not isinstance(n, LeafRef) or n.slot != predicate.name
        ),
    )
    with pytest.raises(ValueError, match="required"):
        prepared.verify(raw, classifier, frame, binding, target=invalid, forms={})


def test_two_traits_are_independent_operands() -> None:
    raw = "自分のデッキから仮族・仮別族・フォロワー7枚を探す。"
    classifier = engine()
    before = derive_source(raw, source(raw), classifier).derived.recognized[0]
    prepared = derive_source(raw, source(raw), classifier)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, classifier, frame, binding)
    expr = next(v for v in binding.values.values() if isinstance(v, PredicateExpr))
    assert expr.expression_id == "traits.all2.v1"
    assert tuple(binding.values[n] for n in expr.leaves) == (
        GlossaryReference(kind="glossary", key="term:trait.synthetic"),
        GlossaryReference(kind="glossary", key="term:trait.other"),
    )
    kind = next(s for s in frame.leaf_schema.slots if s.type == "CardKind")
    assert kind.name not in expr.leaves
    assert kind == next(s for s in before.schema.slots if s.name == kind.name)


@pytest.mark.parametrize(
    "noun",
    [
        "仮族・仮別族・仮族・フォロワー",
        "仮族・仮別族・トークン・フォロワー",
        "仮族・フォロワーか仮別族・フォロワー",
    ],
)
def test_unsealed_trait_combinations_keep_the_original_preparation(noun: str) -> None:
    raw = f"自分のデッキから{noun}7枚を探す。"
    classifier = engine()
    before = derive_source(raw, source(raw), classifier)
    after = derive_source(raw, source(raw), classifier)
    assert after.derived == before.derived
    assert after.interfaces == before.interfaces


def test_departed_event_subject_uses_lki_instead_of_a_selection() -> None:
    raw = "自分のトークン・カードが場を離れたとき、それと同名のカード7枚をEXエリアに置く。"
    classifier = engine()
    prepared = derive_source(raw, source(raw), classifier)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, classifier, frame, binding)
    expr = next(v for v in binding.values.values() if isinstance(v, PredicateExpr))
    assert expr.expression_id == "name.relative.event_subject.equal.v1"
    evidence = frame.projection.imports[0].evidence
    assert evidence is not None
    assert evidence.capture == "lki"
    assert evidence.value.kind == "ObjectSet"
    assert evidence.value.origin == "event_subject"


def test_finite_fragment_or_preserves_repeated_operands() -> None:
    raw = "自分のデッキから「カード名に『仮場手札自分』か『仮場手札自分』を含むフォロワー」7枚を探す。"
    classifier = engine()
    prepared = derive_source(raw, source(raw), classifier)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, classifier, frame, binding)
    expr = next(v for v in binding.values.values() if isinstance(v, PredicateExpr))
    assert expr.expression_id == "name.contains.any2.v1"
    assert len(set(expr.leaves)) == 2
    assert binding.values[expr.leaves[0]] == binding.values[expr.leaves[1]]
    assert len({o.raw_spans for o in binding.occurrences if o.slot in expr.leaves}) == 2


def test_missing_fragment_has_no_literal_or_optional_escape() -> None:
    raw = "自分のデッキから「カード名に『未採用の仮片段』を含むフォロワー」7枚を探す。"
    classifier = engine()
    prepared = derive_source(raw, source(raw), classifier)
    assert "missing_or_ambiguous_name_operand" in prepared.derived.recognized[0].issues
    with pytest.raises(ValueError, match="Unresolved"):
        prepared.bind(0)


def test_missing_kind_keeps_the_two_trait_source_pending() -> None:
    base = engine()
    classifier = Classifier(
        tuple(t for t in base.terms.values() if t.id != "term:object.card"),
        replace(
            base.references,
            terms={k: v for k, v in base.references.terms.items() if k != "カード"},
        ),
        base.rules,
    )
    raw = "自分のデッキから仮族・仮別族・カード7枚を探す。"
    prepared = derive_source(raw, source(raw), classifier)
    assert "missing_name_filter_kind" in prepared.derived.recognized[0].issues
    with pytest.raises(ValueError, match="Unresolved"):
        prepared.bind(0)


def test_repeated_holder_relations_share_one_source_object() -> None:
    raw = "自分のデッキから「これと同名を除く仮族・カード」7枚を探す。自分の墓場の「これと同名でない仮族・カード」7枚を選ぶ。"
    classifier = engine()
    prepared = derive_source(raw, source(raw), classifier)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, classifier, frame, binding)
    ports = frame.projection.imports
    assert len(ports) == 2
    assert ports[0].evidence is not None
    assert ports[1].evidence is not None
    assert ports[0].evidence.value == ports[1].evidence.value
