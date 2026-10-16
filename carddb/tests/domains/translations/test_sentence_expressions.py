"""Selected sentence wording owns expression translations, including variant pins."""

from dataclasses import replace

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.four_layer import LiteralNode, Target
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_authored import (
    TargetVariantRecord,
    from_files,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_matching import Matched
from sve_carddb.domains.translations.four_layer_pipeline import CompiledField
from sve_carddb.domains.translations.four_layer_render import SelectedTarget
from sve_carddb.domains.translations.four_layer_sources import CardSource

from .context_fixtures import contract_case
from .test_four_layer_authored import file
from .test_four_layer_classification import classifier
from .test_four_layer_expressions import EXPRESSION_ID, sample
from .test_four_layer_selection import controls, override


def wording(target: Target, suffix: str) -> Target:
    """Synthetic suffixes distinguish sentence context without changing source parameters."""
    return Target(
        format=1, nodes=(*target.nodes, LiteralNode(kind="Literal", text=suffix))
    )


def test_same_expression_keeps_each_frame_targets_wording() -> None:
    item, renderer, expressions = sample()
    assert item.selected is not None
    base = expressions.targets[EXPRESSION_ID, "zh-Hant"].target
    first = replace(
        item,
        selected=replace(
            item.selected, expression_targets={EXPRESSION_ID: wording(base, "甲")}
        ),
    )
    other_frame = item.frame.model_copy(
        update={"id": "frame:" + "f" * 64, "content_hash": "f" * 64}
    )
    second = replace(
        item,
        frame=other_frame,
        binding=item.binding.model_copy(update={"frame_id": other_frame.id}),
        selected=replace(
            item.selected, expression_targets={EXPRESSION_ID: wording(base, "乙")}
        ),
    )
    one = renderer.render("ctx:first", "zh-Hant", (first,)).rendered
    two = renderer.render("ctx:second", "zh-Hant", (second,)).rendered
    assert one is not None
    assert two is not None
    assert one.text == "自己的手牌張數甲"
    assert two.text == "自己的手牌張數乙"
    assert one.dependency_key != two.dependency_key
    combined = renderer.render(
        "ctx:combined",
        "zh-Hant",
        (
            first,
            replace(second, binding=second.binding.model_copy(update={"ordinal": 1})),
        ),
    ).rendered
    assert combined is not None
    assert combined.text == "自己的手牌張數甲自己的手牌張數乙"
    assert first.selected is not None
    repeated = replace(
        first,
        selected=replace(
            first.selected,
            target=Target(format=1, nodes=first.selected.target.nodes * 2),
        ),
    )
    twice = renderer.render("ctx:twice", "zh-Hant", (repeated,)).rendered
    assert twice is not None
    assert twice.text == "自己的手牌張數甲自己的手牌張數甲"
    assert len({leaf.node_path for leaf in twice.leaves}) == len(twice.leaves)


def test_pin_selects_expression_wording_with_its_sentence_variant() -> None:
    item, renderer, expressions = sample()
    assert item.selected is not None
    base = expressions.targets[EXPRESSION_ID, "zh-Hant"].target
    default = replace(
        item.selected, expression_targets={EXPRESSION_ID: wording(base, "甲")}
    )
    named = TargetVariantRecord.model_validate_json(
        canonical(
            {
                "kind": "template_translation_variant",
                "data": {
                    "template_id": item.frame.id,
                    "lang": "zh-Hant",
                    "variant_key": "alternative",
                    "target": default.target.model_dump(mode="json"),
                    "expression_targets": {
                        EXPRESSION_ID: wording(base, "乙").model_dump(mode="json")
                    },
                },
            }
        )
    )
    source = item.binding.source
    raw = "TEST_SELECT({zone},{kind},{quantity})"
    field = CompiledField(
        CardSource(source, raw, "card", "face", "source"),
        derive_source(raw, source, classifier()),
        (Matched(item.frame, item.binding),),
        (False,),
        (),
    )
    pin = override(
        item.binding,
        "pin",
        [
            {
                "template_id": item.frame.id,
                "lang": "zh-Hant",
                "variant_key": "alternative",
            }
        ],
    )
    chosen = controls(named, pin).select(
        field, renderer, {(item.frame.id, "zh-Hant"): default}, "zh-Hant"
    )
    selected = chosen.targets[item.frame.id, "zh-Hant"]
    assert selected.variant_key == "alternative"
    rendered = chosen.renderer.render(
        "ctx:pinned", "zh-Hant", (replace(item, selected=selected),)
    ).rendered
    assert rendered is not None
    assert rendered.text == "自己的手牌張數乙"
    ordinary = renderer.render(
        "ctx:default", "zh-Hant", (replace(item, selected=default),)
    ).rendered
    assert ordinary is not None
    assert ordinary.text == "自己的手牌張數甲"


@pytest.mark.parametrize("extra", [False, True], ids=["missing", "unused"])
def test_selected_sentence_rejects_missing_or_unused_expression_wording(
    extra: bool,
) -> None:
    item, renderer, expressions = sample()
    assert item.selected is not None
    targets = (
        {
            EXPRESSION_ID: expressions.targets[EXPRESSION_ID, "zh-Hant"].target,
            "unused.v1": Target(
                format=1, nodes=(LiteralNode(kind="Literal", text="未用"),)
            ),
        }
        if extra
        else {}
    )
    plan = (
        replace(
            item,
            selected=SelectedTarget(item.selected.target, expression_targets=targets),
        ),
    )
    if extra:
        with pytest.raises(ValueError, match="Unused expression translation"):
            renderer.render("ctx:unused", "zh-Hant", plan)
    else:
        assert renderer.render("ctx:missing", "zh-Hant", plan).issues == (
            "missing_expression_translation",
        )


@pytest.mark.parametrize("bad", ["unknown", "unused", "formal"])
def test_unselected_variant_is_statically_checked(bad: str) -> None:
    _, _, expressions = sample()
    case = contract_case("A1")
    item = case.item
    assert item.selected is not None
    translations = {
        identifier: value.target
        for (identifier, _), value in case.targets.targets.items()
    }
    if bad == "unknown":
        translations["unknown.v1"] = item.selected.target
    elif bad == "unused":
        translations[EXPRESSION_ID] = expressions.targets[
            EXPRESSION_ID, "zh-Hant"
        ].target
    else:
        identifier = next(iter(translations))
        translations[identifier] = item.selected.target
    record: dict[str, JsonValue] = {
        "kind": "template_translation_variant",
        "data": {
            "template_id": item.frame.id,
            "lang": "zh-Hant",
            "variant_key": "unused_variant",
            "target": item.selected.target.model_dump(mode="json"),
            "expression_targets": {
                k: v.model_dump(mode="json") for k, v in translations.items()
            },
        },
    }
    with pytest.raises(
        ValueError, match=r"unknown source expression|Unused expression|unknown leaf"
    ):
        from_files(
            (
                file(
                    "translations/templates/definitions/001.yaml",
                    [
                        {
                            "kind": "sentence_template",
                            "data": item.frame.model_dump(mode="json"),
                        }
                    ],
                ),
                file("translations/templates/values/001.yaml", [record]),
            )
        )
