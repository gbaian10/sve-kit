"""N1 presentation closure and finite forms exercise real derivation and typed rendering."""

from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.four_layer import (
    CardArgs,
    CardNP,
    FormDefinition,
    FormNode,
    LabelPart,
    LeafRef,
    LiteralNode,
    Signature,
    SlotRef,
    Target,
    ZoneMemberPart,
)
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Form,
    Label,
    Renderer,
    SelectedTarget,
    validate_form,
)

from .test_four_layer_classification import source
from .test_four_layer_n1 import n1_classifier
from .test_four_layer_render import expanded_target, renderer, sample
from .test_parameter_group_render import DATA, render_item
from .test_parameter_group_render import engine as render_engine

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier


@pytest.fixture
def engine() -> Classifier:
    return n1_classifier()


def label_form(identifier: str, name: str, type_name: str, role: str) -> FormDefinition:
    """Synthetic forms use a suffix to expose the boundary of their label ranges."""
    rule = {
        "zone": "zone.case.v2",
        "position": "deck_position.display.v1",
        "phase": "phase.trigger.v1",
        "owner": "owner.possessive.v1",
        "class": "class.filter.v1",
        "trait": "trait.filter.v1",
        "term": "ability.display.v1" if role == "ability" else "keyword.display.v1",
    }[name]
    if identifier in {"zone.add_to_hand", "zone.return_to_hand"}:
        rule = "zone.hand_action.v1"
    cases = (
        (
            "battlefield",
            "hand",
            "deck",
            "evolve_deck",
            "graveyard",
            "ex",
            "banish",
            "mixed",
        )
        if rule == "zone.case.v2"
        else ("default",)
    )
    return FormDefinition.model_validate_json(
        canonical(
            {
                "id": identifier,
                "lang": "zh-Hant",
                "rule": rule,
                "signature": [{"name": name, "type": type_name, "role": role}],
                "cases": {
                    case: [
                        {"kind": "Label", "arg": name},
                        {"kind": "Literal", "text": "的"},
                    ]
                    for case in cases
                },
            }
        )
    )


def synthetic_renderer(
    item: BoundTarget, forms: tuple[FormDefinition, ...] = ()
) -> Renderer:
    """Every label is a test phrase, with quality tied only to used dependencies."""
    result = Renderer(
        {},
        {},
        {(f.id, f.lang): Form(f) for f in forms},
        domains=n1_classifier().domains,
    )
    result.labels = {
        (canonical(ref.model_dump(mode="json")), "zh-Hant"): Label(
            ref, "zh-Hant", "檢驗詞", "project", False, True
        )
        for slot in item.frame.leaf_schema.slots
        for ref in result.references(item, slot.name)
    }
    return result


@pytest.mark.parametrize(
    "raw",
    [
        "自分の手札のフォロワー７枚を選ぶ。",
        "相手のEXエリアのフォロワーが７枚以上なら、仮効果。",
        "自分のデッキの上４枚を見る。",
        "相手のエンドフェイズが来たとき、仮効果。",
        "【コンボ_７】仮効果。",
        "【レッスン_７】仮効果。",
        "【NC_７】仮効果。",
        "【SC_７】仮効果。",
        "仮対象は【仮旗】を持つ。",
        "自分の場か自分のEXエリアのフォロワー７枚を選ぶ。",
    ],
)
def test_required_references_render_by_slot_and_member(
    engine: Classifier, raw: str
) -> None:
    found = derive_source(raw, source(raw), engine)
    frame, binding = found.bind(0)
    target = Target(
        format=1,
        nodes=tuple(
            LeafRef(kind="LeafRef", slot=s.name)
            for s in frame.leaf_schema.slots
            if s.required
        ),
    )
    found.verify(raw, engine, frame, binding, target=target)
    item = BoundTarget(frame, binding, SelectedTarget(target))
    separator = FormDefinition(
        id="zone.union_separator",
        lang="zh-Hant",
        rule="zone.union_separator.v2",
        signature=(),
        cases={"default": (LiteralNode(kind="Literal", text="或"),)},
    )
    selected = synthetic_renderer(item, (separator,))
    output = selected.render("context:closure", "zh-Hant", (item,)).rendered
    assert output is not None
    required = selected.required_references(item, "zh-Hant")
    assert len(required) == len(output.annotation.occurrences)
    assert all(
        output.text[r.start : r.end] == "檢驗詞"
        for a in output.annotation.occurrences
        for r in a.ranges
    )
    if "場か" in raw:
        assert len([r for r in required if r[1] == "source_zone"]) == 2
        assert "或" in output.text
    with pytest.raises(ValueError, match="every required leaf"):
        selected.render(
            "context:closure",
            "zh-Hant",
            (
                replace(
                    item,
                    selected=SelectedTarget(
                        Target(
                            format=1,
                            nodes=(LiteralNode(kind="Literal", text=output.text),),
                        )
                    ),
                ),
            ),
        )


@pytest.mark.parametrize(
    "filter_text",
    ["仮族・", "{仮クラス}", "トークン・", "{仮クラス}仮族・", "トークン・仮族・"],
)
def test_card_np_cannot_leave_operand_filters_outside(
    engine: Classifier, filter_text: str
) -> None:
    raw = (
        "自分のEXエリアの"
        + filter_text
        + "フォロワー７"
        + ("体" if "トークン" in filter_text else "枚")
        + "を選ぶ。"
    )
    found = derive_source(raw, source(raw), engine)
    frame, binding = found.bind(0)
    refs = {s.role: SlotRef(slot=s.name) for s in frame.leaf_schema.slots}
    card = CardNP(
        kind="NP",
        constructor="CardNP",
        args=CardArgs(
            kind=refs["counted_kind"],
            quantity=refs["selection_count"],
            owner=refs["source_owner"],
            zone=refs["source_zone"],
            traits=(refs["trait"],) if "trait" in refs else (),
            **({"class": refs["class_filter"]} if "class_filter" in refs else {}),
            **({"token": refs["token_filter"]} if "token_filter" in refs else {}),
        ),
    )
    found.verify(raw, engine, frame, binding, target=Target(format=1, nodes=(card,)))
    for name in ("traits", "class_", "token"):
        if not getattr(card.args, name):
            continue
        incomplete = card.model_copy(
            update={
                "args": card.args.model_copy(
                    update={name: () if name == "traits" else None}
                )
            }
        )
        flat = tuple(
            LeafRef(kind="LeafRef", slot=s.name) for s in frame.leaf_schema.slots
        )
        with pytest.raises(ValueError, match="supported source operand"):
            found.verify(
                raw,
                engine,
                frame,
                binding,
                target=Target(format=1, nodes=(incomplete, *flat)),
            )
        found.verify(raw, engine, frame, binding, target=Target(format=1, nodes=flat))


@pytest.mark.parametrize(
    "raw",
    [
        "{起動}手札７枚を捨てる:仮効果。",
        "自分の墓場のフォロワー７枚を選び、手札に加える。",
        "自分のデッキの上４枚を見る。残りをデッキの下に戻す。",
    ],
)
def test_unused_omitted_owner_has_no_label_or_quality_dependency(
    engine: Classifier, raw: str
) -> None:
    found = derive_source(raw, source(raw), engine)
    frame, binding = found.bind(0)
    optional = next(s for s in frame.leaf_schema.slots if not s.required)
    nodes = tuple(
        LeafRef(kind="LeafRef", slot=s.name)
        for s in frame.leaf_schema.slots
        if s.required
    )
    item = BoundTarget(frame, binding, SelectedTarget(Target(format=1, nodes=nodes)))
    selected = synthetic_renderer(item)
    refs = selected.references(item, optional.name)
    for ref in refs:
        selected.labels[canonical(ref.model_dump(mode="json")), "zh-Hant"] = Label(
            ref, "zh-Hant", "檢驗持有者", "machine", True, True
        )
    before = selected.render("context:omitted", "zh-Hant", (item,)).rendered
    assert before is not None
    for ref in refs:
        del selected.labels[canonical(ref.model_dump(mode="json")), "zh-Hant"]
    # A label shared with a separate explicit owner is still required for that owner.
    if any(
        selected.references(item, s.name) == refs
        for s in frame.leaf_schema.slots
        if s.required
    ):
        assert selected.render("context:omitted", "zh-Hant", (item,)).issues == (
            "missing_term_translation",
        )
    else:
        after = selected.render("context:omitted", "zh-Hant", (item,)).rendered
        assert after == before
    displayed = replace(
        item,
        selected=SelectedTarget(
            Target(
                format=1, nodes=(*nodes, LeafRef(kind="LeafRef", slot=optional.name))
            )
        ),
    )
    assert selected.render("context:omitted", "zh-Hant", (displayed,)).issues == (
        "missing_term_translation",
    )


@pytest.mark.parametrize("quantity_role", ["selection_count", "existence_count"])
def test_owner_only_counted_np_uses_possessive_and_counted_classifier(
    quantity_role: str,
) -> None:
    fixture = deepcopy(object_value(object_value(DATA["fixtures"])["g07"]))
    semantic = object_value(fixture["semantic"])
    schema = object_value(semantic["leaf_schema"])
    for value in array(schema["slots"]):
        slot = object_value(value)
        role = string(slot["role"])
        slot["role"] = {
            "source_zone": "counted_zone",
            "source_owner": "counted_owner",
            "selection_count": quantity_role,
        }.get(role, role)
    item = render_item(fixture)
    nodes = array(object_value(fixture["np_target"])["nodes"])
    card = next(
        object_value(n) for n in nodes if object_value(n).get("constructor") == "CardNP"
    )
    object_value(card["args"]).pop("zone")
    nodes.insert(0, {"kind": "LeafRef", "slot": "zone"})
    item = replace(
        item,
        selected=SelectedTarget(
            Target.model_validate_json(canonical(fixture["np_target"]))
        ),
    )
    selected = render_engine()
    classifier = selected.forms.pop(
        ("quantity.classifier.source_selection", "zh-Hant")
    ).definition.model_dump(mode="json")
    classifier["id"] = (
        "quantity.classifier.counted_selection"
        if quantity_role == "selection_count"
        else "quantity.classifier.counted_existence"
    )
    for arg in classifier["signature"]:
        arg["role"] = {
            "source_zone": "counted_zone",
            "selection_count": quantity_role,
        }.get(arg["role"], arg["role"])
    f = FormDefinition.model_validate_json(canonical(classifier))
    owner = label_form("owner.counted_possessive", "owner", "Player", "counted_owner")
    selected.forms[f.id, f.lang] = Form(f)
    selected.forms[owner.id, owner.lang] = Form(owner)
    output = selected.render("context:owner", "zh-Hant", (item,)).rendered
    assert output is not None
    assert "自己的2張從者" in output.text
    assert (
        "的"
        not in output.text[
            output.annotation.occurrences[1]
            .ranges[0]
            .start : output.annotation.occurrences[1].ranges[0].end
        ]
    )
    del selected.forms[owner.id, owner.lang]
    assert selected.render("context:owner", "zh-Hant", (item,)).issues == (
        "missing_owner_possessive_form",
    )


def test_classifier_arguments_cannot_complete_required_leaf_references() -> None:
    original = expanded_target()
    item = sample(
        Target(
            format=1,
            nodes=tuple(
                n
                for n in original.nodes
                if not isinstance(n, FormNode) or n.form_id != "zone.counted_locative"
            ),
        )
    )
    with pytest.raises(ValueError, match="every required leaf"):
        renderer().render("context:classifier", "zh-Hant", (item,))


@pytest.mark.parametrize("field", ["signature", "cases", "rule"])
def test_separator_is_the_only_literal_constant_form(field: str) -> None:
    definition: dict[str, JsonValue] = {
        "id": "zone.union_separator",
        "lang": "zh-Hant",
        "rule": "zone.union_separator.v2",
        "signature": [],
        "cases": {"default": [{"kind": "Literal", "text": "或"}]},
    }
    validate_form(FormDefinition.model_validate_json(canonical(definition)))
    if field == "signature":
        definition[field] = [{"name": "zone", "type": "ZoneSet", "role": "source_zone"}]
    elif field == "cases":
        object_value(definition[field])["extra"] = [{"kind": "Literal", "text": "或"}]
    else:
        definition[field] = "zone.union_separator.v1"
    with pytest.raises(
        ValueError, match=r"empty signature|registered rule cases|typed role signature"
    ):
        validate_form(FormDefinition.model_validate_json(canonical(definition)))
    definition.update(
        id="owner.source_possessive", rule="owner.possessive.v1", signature=[]
    )
    with pytest.raises(
        ValueError, match=r"empty signature|registered rule cases|typed role signature"
    ):
        validate_form(FormDefinition.model_validate_json(canonical(definition)))


@pytest.mark.parametrize(
    ("identifier", "name", "type_name", "role"),
    [
        ("zone.locative", "zone", "ZoneSet", "source_zone"),
        ("zone.counted_locative", "zone", "ZoneSet", "counted_zone"),
        ("zone.allative", "zone", "ZoneSet", "destination_zone"),
        ("zone.ablative", "zone", "ZoneSet", "source_zone"),
        ("zone.add_to_hand", "zone", "ZoneSet", "destination_zone"),
        ("zone.return_to_hand", "zone", "ZoneSet", "destination_zone"),
        ("deck_position.ablative", "position", "DeckPosition", "source_position"),
        ("deck_position.allative", "position", "DeckPosition", "destination_position"),
        ("phase.trigger", "phase", "Phase", "phase_trigger"),
        ("ability.display", "term", "Concept", "ability"),
        ("keyword.display", "term", "Concept", "keyword"),
        ("trait.filter", "trait", "Concept", "trait"),
        ("class.filter", "class", "Concept", "class_filter"),
        ("owner.source_possessive", "owner", "Player", "source_owner"),
        ("owner.counted_possessive", "owner", "Player", "counted_owner"),
    ],
)
def test_label_forms_have_closed_roles_and_always_present_reference(
    identifier: str, name: str, type_name: str, role: str
) -> None:
    definition = label_form(identifier, name, type_name, role)
    validate_form(definition)
    changed = definition.model_dump(mode="json")
    changed["signature"][0]["role"] = "synthetic_role"
    with pytest.raises(ValueError, match="typed role signature"):
        validate_form(FormDefinition.model_validate_json(canonical(changed)))
    changed = definition.model_dump(mode="json")
    for parts in changed["cases"].values():
        parts[:] = [{"kind": "Literal", "text": "假引用"}]
    with pytest.raises(ValueError, match="must present its reference"):
        validate_form(FormDefinition.model_validate_json(canonical(changed)))


@pytest.mark.parametrize(
    ("raw", "identifier"),
    [("手札に加える。", "zone.add_to_hand"), ("手札に戻す。", "zone.return_to_hand")],
)
def test_hand_action_forms_require_the_same_registered_local_action(
    engine: Classifier, raw: str, identifier: str
) -> None:
    found = derive_source(raw, source(raw), engine)
    frame, binding = found.bind(0)
    node = FormNode(
        kind="Form", form_id=identifier, args={"zone": SlotRef(slot="leaf_0")}
    )
    found.verify(raw, engine, frame, binding, target=Target(format=1, nodes=(node,)))
    wrong = node.model_copy(
        update={
            "form_id": "zone.return_to_hand"
            if identifier == "zone.add_to_hand"
            else "zone.add_to_hand"
        }
    )
    with pytest.raises(ValueError, match="registered local source action"):
        found.verify(
            raw, engine, frame, binding, target=Target(format=1, nodes=(wrong,))
        )


def test_normal_source_flat_form_and_np_preserve_frame_binding_and_ranges(
    engine: Classifier,
) -> None:
    raw = "自分の手札のフォロワー７枚を選ぶ。"
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    refs = {slot.role: SlotRef(slot=slot.name) for slot in frame.leaf_schema.slots}
    zone = label_form("zone.locative", "zone", "ZoneSet", "source_zone")
    classifier = (
        render_engine()
        .forms["quantity.classifier.source_selection", "zh-Hant"]
        .definition
    )
    owner = LeafRef(kind="LeafRef", slot=refs["source_owner"].slot)
    count = LeafRef(kind="LeafRef", slot=refs["selection_count"].slot)
    kind = LeafRef(kind="LeafRef", slot=refs["counted_kind"].slot)
    unit = FormNode(
        kind="Form",
        form_id=classifier.id,
        args={
            "kind": refs["counted_kind"],
            "quantity": refs["selection_count"],
            "zone": refs["source_zone"],
        },
    )
    targets = (
        Target(
            format=1,
            nodes=(
                owner,
                LeafRef(kind="LeafRef", slot=refs["source_zone"].slot),
                LiteralNode(kind="Literal", text="的"),
                count,
                unit,
                kind,
            ),
        ),
        Target(
            format=1,
            nodes=(
                owner,
                FormNode(
                    kind="Form", form_id=zone.id, args={"zone": refs["source_zone"]}
                ),
                count,
                unit,
                kind,
            ),
        ),
        Target(
            format=1,
            nodes=(
                CardNP(
                    kind="NP",
                    constructor="CardNP",
                    args=CardArgs(
                        owner=refs["source_owner"],
                        zone=refs["source_zone"],
                        quantity=refs["selection_count"],
                        kind=refs["counted_kind"],
                    ),
                ),
            ),
        ),
    )
    outputs = []
    for target in targets:
        derived.verify(raw, engine, frame, binding, target=target)
        item = BoundTarget(frame, binding, SelectedTarget(target))
        output = (
            synthetic_renderer(item, (zone, classifier))
            .render("context:styles", "zh-Hant", (item,))
            .rendered
        )
        assert output is not None
        outputs.append(output)
    assert len({output.text for output in outputs}) == 1
    assert (
        len(
            {canonical(output.annotation.model_dump(mode="json")) for output in outputs}
        )
        == 1
    )
    assert len({output.dependency_key for output in outputs}) == 3
    assert derived.bind(0) == (frame, binding)


@pytest.mark.parametrize("role", ["existence_count", "counter_amount"])
def test_np_rejects_unregistered_source_classifier_tuple(role: str) -> None:
    fixture = deepcopy(object_value(object_value(DATA["fixtures"])["g07"]))
    schema = object_value(object_value(fixture["semantic"])["leaf_schema"])
    for value in array(schema["slots"]):
        slot = object_value(value)
        if slot["role"] == "selection_count":
            slot["role"] = role
    item = replace(
        render_item(fixture),
        selected=SelectedTarget(
            Target.model_validate_json(canonical(fixture["np_target"]))
        ),
    )
    with pytest.raises(ValueError, match=r"unsupported classifier|supported quantity"):
        render_engine().render("context:unsupported", "zh-Hant", (item,))


def source_union_form(suffix: str) -> FormDefinition:
    return FormDefinition(
        id="zone.source_union." + suffix,
        lang="zh-Hant",
        rule="zone.source_union.v1",
        signature=(
            Signature(name="owner", type="Player", role="source_owner"),
            Signature(name="zone", type="ZoneSet", role="source_zone"),
        ),
        cases={
            "battlefield": (
                LabelPart(kind="Label", arg="owner"),
                ZoneMemberPart(kind="ZoneMember", arg="zone"),
                *(
                    (LiteralNode(kind="Literal", text="上"),)
                    if suffix == "locative_or"
                    else ()
                ),
            ),
            "ex": (
                LabelPart(kind="Label", arg="owner"),
                ZoneMemberPart(kind="ZoneMember", arg="zone"),
            ),
            "separator": (
                LiteralNode(kind="Literal", text="與" if suffix == "and" else "或"),
            ),
        },
    )


@pytest.mark.parametrize("suffix", ["locative_or", "or", "and"])
@pytest.mark.parametrize("connector", ["か", "や"])
@pytest.mark.parametrize("reverse", [False, True])
def test_source_union_preserves_member_ranges_and_shared_owner(
    engine: Classifier, suffix: str, connector: str, *, reverse: bool
) -> None:
    zones = ("EXエリア", "場") if reverse else ("場", "EXエリア")
    raw = f"自分の{zones[0]}{connector}自分の{zones[1]}のフォロワー７枚を選ぶ。"
    found = derive_source(raw, source(raw), engine)
    frame, binding = found.bind(0)
    slots = {s.role: s.name for s in frame.leaf_schema.slots}
    form = source_union_form(suffix)
    target = Target(
        format=1,
        nodes=(
            FormNode(
                kind="Form",
                form_id=form.id,
                args={
                    "owner": SlotRef(slot=slots["source_owner"]),
                    "zone": SlotRef(slot=slots["source_zone"]),
                },
            ),
            LeafRef(kind="LeafRef", slot=slots["counted_kind"]),
            LeafRef(kind="LeafRef", slot=slots["selection_count"]),
        ),
    )
    found.verify(raw, engine, frame, binding, target=target)
    item = BoundTarget(frame, binding, SelectedTarget(target))
    selected = synthetic_renderer(item, (form,))
    names = {
        "term:player.self": "自己",
        "term:zone.battlefield": "戰場",
        "term:zone.ex": "EX區",
    }
    selected.labels = {
        key: replace(
            label, text=names.get(getattr(label.reference, "key", ""), "仮從者")
        )
        for key, label in selected.labels.items()
    }
    output = selected.render("context:source-union", "zh-Hant", (item,)).rendered
    assert output is not None
    connector_text = "與" if suffix == "and" else "或"
    suffix_text = "上" if suffix == "locative_or" else ""
    assert output.text == f"自己戰場{suffix_text}{connector_text}自己EX區仮從者7"
    assert [
        output.text[r.start : r.end]
        for a in output.annotation.occurrences
        for r in a.ranges
    ] == ["自己", "戰場", "自己", "EX區", "仮從者"]
    zone_outputs = [leaf for leaf in output.leaves if leaf.slot == slots["source_zone"]]
    assert [
        output.text[r.start : r.end] for leaf in zone_outputs for r in leaf.ranges
    ] == ["戰場", "EX區"]
    assert len({leaf.node_path for leaf in zone_outputs}) == len(zone_outputs)
    assert {o.slot for o in binding.occurrences} == set(slots.values())


@pytest.mark.parametrize("connector", ["と", "または", "および"])
def test_source_union_does_not_admit_unregistered_conjunction(
    engine: Classifier, connector: str
) -> None:
    raw = f"自分の場{connector}自分のEXエリアのフォロワー７枚を選ぶ。"
    found = derive_source(raw, source(raw), engine)
    assert all(
        slot.domain.values != ("zone.selection.battlefield_ex.v1",)
        for slot in found.recognized[0].schema.slots
    )


def test_source_union_rejects_owner_from_another_operand(engine: Classifier) -> None:
    raw = "自分の場か自分のEXエリアのフォロワー７枚を選ぶ。相手の場や相手のEXエリアのフォロワー７枚を選ぶ。"
    found = derive_source(raw, source(raw), engine)
    frame, binding = found.bind(0)
    owners = [s.name for s in frame.leaf_schema.slots if s.role == "source_owner"]
    zones = [s.name for s in frame.leaf_schema.slots if s.role == "source_zone"]
    target = Target(
        format=1,
        nodes=(
            FormNode(
                kind="Form",
                form_id="zone.source_union.and",
                args={
                    "owner": SlotRef(slot=owners[0]),
                    "zone": SlotRef(slot=zones[-1]),
                },
            ),
        ),
    )
    with pytest.raises(ValueError, match="one registered G10 operand"):
        found.verify(raw, engine, frame, binding, target=target)


@pytest.mark.parametrize("case", ["battlefield", "ex"])
def test_source_union_rejects_missing_member_or_owner(case: str) -> None:
    form = source_union_form("or")
    for missing in ("Label", "ZoneMember"):
        broken = form.model_copy(
            update={
                "cases": form.cases
                | {
                    case: tuple(p for p in form.cases[case] if p.kind != missing),
                }
            }
        )
        with pytest.raises(ValueError, match="present owner and member"):
            validate_form(broken)
