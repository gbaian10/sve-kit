"""N1 source rules derive values and reject neighboring constructions through the public entry."""

from typing import TYPE_CHECKING, cast

import pytest

from sve_carddb.contracts.four_layer import (
    CardArgs,
    CardNP,
    GlossaryReference,
    LeafSlot,
    SlotRef,
    Target,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.contracts.source_binding import verify_value

if TYPE_CHECKING:
    from sve_carddb.contracts.source_binding import TypedValue
from sve_carddb.domains.catalog.models import Term as VocabularyTerm
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.vocabulary import Binding, Vocabulary
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term
from sve_carddb.domains.translations.four_layer_derivation import (
    DerivedField,
    derive_source,
)
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.recognition.rules import Rules

from .test_four_layer_classification import classifier, source
from .test_four_layer_derivation import context


@pytest.fixture
def engine() -> Classifier:
    terms = (
        Term("term:ability.quick", "ability", "クイック"),
        Term("term:keyword.synthetic", "keyword", "仮旗"),
        Term("term:object.card", "rule_term", "カード"),
        Term("term:trait.synthetic", "trait", "仮族"),
        Term("term:name.synthetic", "card_name", "仮場手札自分"),
        Term("term:ability.activation", "ability", "起動"),
        Term("term:ability.fanfare", "ability", "ファンファーレ"),
        *(
            Term("term:ability." + code, "ability", raw)
            for code, raw in (
                ("combo", "コンボ"),
                ("lesson", "レッスン"),
                ("necrocharge", "ネクロチャージ"),
                ("spell_chain", "スペルチェイン"),
            )
        ),
    )
    result = classifier(
        terms,
        (
            "braced_ability_reference",
            "bracket_keyword_reference",
            "keyword_threshold_combo",
            "keyword_threshold_lesson",
            "keyword_threshold_necrocharge",
            "keyword_threshold_spell_chain",
            "keyword_alias_nc",
            "keyword_alias_sc",
        ),
    )
    members = (
        ("type", "follower", "フォロワー"),
        ("type", "amulet", "アミュレット"),
        ("type", "spell", "スペル"),
        ("class", "synthetic", "仮クラス"),
    )
    result.references.vocabulary = Vocabulary(
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
    )
    return result


def derive(raw: str, engine: Classifier) -> DerivedField:
    found = derive_source(raw, source(raw), engine)
    for index, part in enumerate(found.field.parts):
        if not found.recognized[index].issues:
            frame, binding = found.bind(index)
            found.verify(raw, engine, frame, binding)
            engine.verify(raw, found.field, part, frame, binding)
    return found


def roles(found: DerivedField, ordinal: int = 0) -> dict[str, list[object]]:
    recognized = found.recognized[ordinal]
    result: dict[str, list[object]] = {}
    for slot in recognized.schema.slots:
        result.setdefault(slot.role, []).append(recognized.values[slot.name])
    return result


@pytest.mark.parametrize("owner", ["自分", "相手"])
@pytest.mark.parametrize(
    ("case", "verb"),
    [
        ("の", "選ぶ"),
        ("から", "選ぶ"),
        ("の", "探す"),
        ("から", "見る"),
        ("の", "公開する"),
    ],
)
def test_n1_source_selection_registry(
    engine: Classifier, owner: str, case: str, verb: str
) -> None:
    found = derive(f"{owner}の手札{case}仮族・フォロワー７枚を{verb}。", engine)
    assert found.recognized[0].issues == ()
    assert roles(found)["source_zone"] == [("hand",)]
    assert roles(found)["source_owner"] == ["self" if owner == "自分" else "opponent"]
    slots = found.recognized[0].schema.slots
    assert slots[0].domain.values == ("player.relative.v1",)
    assert roles(found)["counted_kind"] == [
        VocabularyReference(kind="vocabulary", key=("type", "follower"))
    ]
    assert roles(found)["trait"] == [
        GlossaryReference(kind="glossary", key="term:trait.synthetic")
    ]
    assert found.recognized[0].semantics is None


@pytest.mark.parametrize(
    "modifier",
    ["表向きの", "他の", "コスト６以下の", "「相手の墓場を確認する仮条件」の"],
)
def test_n1_s14_retains_modifier_and_outer_count(
    engine: Classifier, modifier: str
) -> None:
    raw = "自分のEXエリアの" + modifier + "カードが７枚以上なら、仮効果。"
    found = derive(raw, engine)
    assert roles(found)["counted_zone"] == [("ex",)]
    assert roles(found)["counted_owner"] == ["self"]
    assert len(roles(found).get("source_zone", [])) == 0
    canonical = found.field.parts[0].canonical_source
    assert (modifier.replace("６", "N") if "６" in modifier else modifier) in canonical
    assert "以上なら" in canonical
    assert "N1-SRC04.quantity" in found.registry_rows[0]


@pytest.mark.parametrize(
    ("tail", "case"),
    [
        ("の枚数", "の"),
        ("の数", "の"),
        ("がいるなら", "に"),
        ("がいないなら", "に"),
        ("があるなら", "に"),
        ("がないなら", "に"),
    ],
)
def test_n1_current_count_registry(engine: Classifier, tail: str, case: str) -> None:
    found = derive("相手の場" + case + "他のフォロワー" + tail + "、仮効果。", engine)
    assert roles(found)["counted_zone"] == [("battlefield",)]
    assert roles(found)["counted_owner"] == ["opponent"]


def test_n1_source_move_no_and_destination_without_owner(engine: Classifier) -> None:
    raw = "自分のエボルヴデッキの『仮場手札自分』７枚をEXエリアに置いてよい。"
    found = derive(raw, engine)
    assert roles(found)["source_zone"] == [("evolve_deck",)]
    assert roles(found)["destination_zone"] == [("ex",)]
    assert "destination_owner" not in roles(found)
    assert "置いてよい" in found.field.parts[0].canonical_source
    assert (
        sum(slot.type == "CardName" for slot in found.recognized[0].schema.slots) == 1
    )
    assert found.recognized[0].semantics is None


def test_n1_deck_edges_and_mill(engine: Classifier) -> None:
    found = derive("自分のデッキの上７枚を墓場に置く。", engine)
    assert roles(found)["source_position"] == ["top"]
    assert roles(found)["destination_zone"] == [("graveyard",)]
    assert "source_zone" not in roles(found)
    found = derive("自分のデッキの上７枚を見る。残りをデッキの下に置く。", engine)
    assert roles(found)["source_position"] == ["top"]
    assert roles(found)["destination_position"] == ["bottom"]
    assert roles(found)["destination_owner"] == ["self"]


@pytest.mark.parametrize("tail", ["が来たとき、仮効果。", "にカードを引く。"])
def test_n1_phase_registry(engine: Classifier, tail: str) -> None:
    found = derive("次の相手のエンドフェイズ" + tail, engine)
    assert roles(found)["phase_trigger"] == ["end"]
    assert roles(found)["phase_owner"] == ["opponent"]
    assert "次の" in found.field.parts[0].canonical_source
    assert tail in found.field.parts[0].canonical_source


@pytest.mark.parametrize(
    "raw",
    [
        "自分のターンに手札を確認する。",
        "相手のリーダーが場に出たとき、仮効果。",
        "自分の場のカード７枚が墓場に置かれたとき、仮効果。",
        "自分の場にフォロワーがいる限り、仮効果。",
        "自分の墓場のカード７枚を踊る。",
        "自分のエンドフェイズ開始時、仮効果。",
        "自分のデッキの上から７枚を見る。",
        "自分の場のフォロワー７体を選んだとき、仮効果。",
    ],
)
def test_n1_source_exclusions(engine: Classifier, raw: str) -> None:
    found = derive(raw, engine)
    assert all(
        slot.type not in {"ZoneSet", "Player", "Phase", "DeckPosition"}
        for slot in found.recognized[0].schema.slots
    )


def test_n1_subject_local_exclusion(engine: Classifier) -> None:
    found = derive("相手は『仮場手札自分』を場に出す。", engine)
    assert roles(found)["destination_zone"] == [("battlefield",)]
    assert "destination_owner" not in roles(found)
    found = derive("相手は手札７枚を捨てる。", engine)
    assert "source_owner" not in roles(found)
    assert "source_zone" not in roles(found)


@pytest.mark.parametrize("join", ["。それを", "、"])
def test_n1_self_hand_guard(engine: Classifier, join: str) -> None:
    verb = "選ぶ" if join == "。それを" else "選び"
    found = derive(
        "自分の墓場のフォロワー７枚を" + verb + join + "手札に加える。", engine
    )
    assert roles(found)["source_owner"] == ["self"]
    assert roles(found)["destination_owner"] == ["self"]
    omitted = tuple(
        o for o in found.recognized[0].occurrences if o.source_presence == "omitted"
    )
    assert len(omitted) == 1
    assert omitted[0].raw_spans == omitted[0].canonical_spans == ()
    assert omitted[0].resolution_rule == "owner.self_hand_destination.v1"
    slot = next(
        s for s in found.recognized[0].schema.slots if s.name == omitted[0].slot
    )
    assert slot.required is False
    assert slot.domain.values == ("player.self.v1",)
    assert all(s.required for s in found.recognized[0].schema.slots if s.occurrences)


@pytest.mark.parametrize("prefix", ["相手の", ""])
def test_n1_unknown_or_opponent_source_does_not_supply_hand_owner(
    engine: Classifier, prefix: str
) -> None:
    found = derive(prefix + "墓場のフォロワー７枚を選ぶ。それを手札に加える。", engine)
    assert "destination_owner" not in roles(found)
    assert roles(found)["destination_zone"] == [("hand",)]


@pytest.mark.parametrize(
    "gap", ["\nそれを", "。そのカードを", "。相手の手札のカード１枚を選ぶ。それを"]
)
def test_n1_pronoun_does_not_cross_frames_or_other_antecedents(
    engine: Classifier, gap: str
) -> None:
    found = derive("自分の墓場のフォロワー７枚を選ぶ" + gap + "手札に加える。", engine)
    assert all(
        "destination_owner" not in roles(found, i) for i in range(len(found.recognized))
    )


def test_n1_actor_hand_cost_omitted_value_is_still_required_in_binding(
    engine: Classifier,
) -> None:
    raw = "{起動}手札７枚を捨てる:仮効果。"
    found = derive(raw, engine)
    assert roles(found)["source_owner"] == ["self"]
    frame, binding = found.bind(0)
    omitted = next(o for o in binding.occurrences if o.source_presence == "omitted")
    assert binding.occurrences.index(omitted) < next(
        i
        for i, o in enumerate(binding.occurrences)
        if o.slot == frame.leaf_schema.slots[1].name
    )
    modified = binding.model_copy(
        update={
            "values": {k: v for k, v in binding.values.items() if k != omitted.slot}
        }
    )
    modified = modified.model_copy(
        update={"id": "bind:" + hash_payload(modified.payload())}
    )
    with pytest.raises(ValueError, match="differs"):
        found.verify(raw, engine, frame, modified)
    forged = binding.model_copy(
        update={
            "occurrences": tuple(
                o.model_copy(
                    update={"resolution_rule": "owner.same_operand_in_line.v1"}
                )
                if o == omitted
                else o
                for o in binding.occurrences
            )
        }
    )
    forged = forged.model_copy(update={"id": "bind:" + hash_payload(forged.payload())})
    with pytest.raises(ValueError, match="differs"):
        found.verify(raw, engine, frame, forged)


@pytest.mark.parametrize("connector", ["か", "や"])
@pytest.mark.parametrize("first", ["場", "EXエリア"])
@pytest.mark.parametrize("repeat", [False, True])
def test_n1_g10_union_trace(
    engine: Classifier, connector: str, first: str, repeat: bool
) -> None:
    second = "EXエリア" if first == "場" else "場"
    raw = (
        "自分の"
        + first
        + connector
        + ("自分の" if repeat else "")
        + second
        + "の仮族・フォロワー７枚を選ぶ。"
    )
    found = derive(raw, engine)
    assert found.recognized[0].issues == ()
    assert roles(found)["source_zone"] == [("battlefield", "ex")]
    owner = found.recognized[0].schema.slots[0]
    owners = tuple(o for o in found.recognized[0].occurrences if o.slot == owner.name)
    assert len(owners) == (2 if repeat else 1)
    assert len(owner.occurrences) == 1
    zone = next(
        o
        for o in found.recognized[0].occurrences
        if o.slot == found.recognized[0].schema.slots[1].name
    )
    assert len(zone.raw_spans) == 2
    common = derive("自分の場かEXエリアの仮族・フォロワー７枚を選ぶ。", engine)
    assert (
        found.field.parts[0].canonical_source == common.field.parts[0].canonical_source
    )
    assert found.recognized[0].schema == common.recognized[0].schema
    assert found.recognized[0].values == common.recognized[0].values


@pytest.mark.parametrize(
    "scope",
    [
        "場とEXエリア",
        "場またはEXエリア",
        "場や相手のEXエリア",
        "場のフォロワーかEXエリアのスペル",
        "場かEXエリアの各自のフォロワー",
    ],
)
def test_n1_invalid_union_never_matches_its_right_half(
    engine: Classifier, scope: str
) -> None:
    found = derive("自分の" + scope + "のフォロワー７枚を選ぶ。", engine)
    assert "source_zone" not in roles(found)


@pytest.mark.parametrize(
    ("alias", "full", "code", "rule"),
    [
        ("NC", "ネクロチャージ", "necrocharge", "keyword_threshold_necrocharge"),
        ("SC", "スペルチェイン", "spell_chain", "keyword_threshold_spell_chain"),
        ("コンボ", "コンボ", "combo", "keyword_threshold_combo"),
        ("レッスン", "レッスン", "lesson", "keyword_threshold_lesson"),
    ],
)
def test_n1_threshold_names_and_alias(
    engine: Classifier, alias: str, full: str, code: str, rule: str
) -> None:
    short = derive("【" + alias + "_７】仮効果。", engine)
    complete = derive("【" + full + "_７】仮効果。", engine)
    assert short.recognized[0].issues == complete.recognized[0].issues == ()
    assert (
        short.field.parts[0].canonical_source
        == complete.field.parts[0].canonical_source
    )
    assert short.recognized[0].schema == complete.recognized[0].schema
    assert roles(short)["ability"] == [
        GlossaryReference(kind="glossary", key="term:ability." + code)
    ]
    assert roles(short)["threshold"] == [7]
    engine.rules = engine.rules.model_copy(
        update={
            "rules": tuple(
                r.model_copy(update={"enabled": False}) if r.rule_id == rule else r
                for r in engine.rules.rules
            )
        }
    )
    disabled = derive_source(
        "【" + full + "_７】仮効果。", source("【" + full + "_７】仮効果。"), engine
    )
    assert "ability" not in roles(disabled)


def test_n1_keyword_switch_quality_and_fixed_structure(engine: Classifier) -> None:
    engine.rules = engine.rules.model_copy(
        update={
            "rules": tuple(
                r.model_copy(update={"low_confidence": True})
                if r.rule_id == "bracket_keyword_reference"
                else r
                for r in engine.rules.rules
            )
        }
    )
    found = derive("【仮旗】", engine)
    assert found.recognized[0].low_confidence
    assert found.recognized[0].semantics is not None
    engine.rules = Rules(format=2, kind="template_parameter_rules", rules=())
    found = derive_source("【仮旗】", source("【仮旗】"), engine)
    assert found.recognized[0].semantics is None
    assert found.recognized[0].schema.slots == ()
    found = derive("場に出す。", engine)
    assert roles(found)["destination_zone"] == [("battlefield",)]


def test_n1_quick_first_line_guard_and_empty_selection(engine: Classifier) -> None:
    for raw, resolved in (
        ("{クイック}", True),
        ("\n{クイック}", True),
        ("仮効果。\n{クイック}", False),
    ):
        facts = context(raw)
        found = derive_source(raw, facts.source, engine, context=facts)
        part = next(
            i
            for i, p in enumerate(found.field.parts)
            if p.canonical_source == "{クイック}"
        )
        assert (found.recognized[part].semantics is not None) is resolved
    engine.rules = Rules(format=2, kind="template_parameter_rules", rules=())
    raw = "{クイック}"
    found = derive_source(raw, context(raw).source, engine, context=context(raw))
    assert found.recognized[0].semantics is None
    assert found.recognized[0].schema.slots == ()


def test_n1_selection_aliases_and_repeated_operands(engine: Classifier) -> None:
    samples = [
        derive(raw, engine)
        for raw in (
            "自分の墓場のフォロワーを７枚まで選ぶ。",
            "自分の墓場からフォロワー７枚までを選ぶ。",
            "自分の墓場のフォロワー７枚まで選ぶ。",
        )
    ]
    assert len({f.field.parts[0].canonical_source for f in samples}) == 1
    assert all(
        f.recognized[0].schema == samples[0].recognized[0].schema for f in samples
    )
    found = derive(
        "自分の墓場のフォロワー７枚を選ぶ。自分の墓場のフォロワー７枚を選ぶ。", engine
    )
    assert roles(found)["source_owner"] == ["self", "self"]
    assert roles(found)["source_zone"] == [("graveyard",), ("graveyard",)]


def test_n1_matching_keeps_independent_empty_span_slots(engine: Classifier) -> None:
    raw = (
        "{起動}手札７枚を捨てる:自分の墓場のフォロワー７枚を選ぶ。それを手札に加える。"
    )
    found = derive(raw, engine)
    frame, _ = found.bind(0)
    assert sum(not s.required for s in frame.leaf_schema.slots) == 2
    matched = Frames((frame,), normalizer_version="four-layer-jp-v2").match(
        raw, found.field, found.field.parts[0], found.recognized[0], engine
    )
    assert matched is not None
    assert list(matched.binding.values.values()).count("self") == 3


@pytest.mark.parametrize(
    ("domain", "type_name", "valid", "invalid"),
    [
        ("player.self.v1", "Player", "self", "opponent"),
        ("zone.single.v1", "ZoneSet", ("deck",), ("deck", "hand")),
        (
            "zone.selection.battlefield_ex.v1",
            "ZoneSet",
            ("battlefield", "ex"),
            ("battlefield",),
        ),
        ("deck_position.edge.v1", "DeckPosition", "top", "deck_top"),
    ],
)
def test_n1_closed_domains(
    engine: Classifier, domain: str, type_name: str, valid: object, invalid: object
) -> None:
    slot = LeafSlot.model_validate(
        {
            "name": "leaf_0",
            "type": type_name,
            "role": "synthetic",
            "domain": {"values": (domain,), "min": None, "max": None},
            "required": True,
            "occurrences": ({"start": 0, "end": 1},),
        }
    )

    verify_value(slot, cast("TypedValue", valid), engine.domains)
    with pytest.raises(ValueError, match="source domain"):
        verify_value(slot, cast("TypedValue", invalid), engine.domains)


def test_n1_card_np_owner_roles_and_same_operand(engine: Classifier) -> None:

    raw = "自分の墓場のフォロワー７枚を選ぶ。相手の墓場のフォロワー７枚を選ぶ。"
    found = derive(raw, engine)
    frame, binding = found.bind(0)
    first, second = (frame.leaf_schema.slots[:4], frame.leaf_schema.slots[4:8])
    valid = CardNP(
        kind="NP",
        constructor="CardNP",
        args=CardArgs(
            kind=SlotRef(slot=first[2].name),
            quantity=SlotRef(slot=first[3].name),
            owner=SlotRef(slot=first[0].name),
            zone=SlotRef(slot=first[1].name),
        ),
    )
    found.verify(raw, engine, frame, binding, target=Target(format=1, nodes=(valid,)))
    borrowed = valid.model_copy(
        update={
            "args": valid.args.model_copy(
                update={"owner": SlotRef(slot=second[0].name)}
            )
        }
    )
    with pytest.raises(ValueError, match="one supported source operand"):
        found.verify(
            raw, engine, frame, binding, target=Target(format=1, nodes=(borrowed,))
        )
    for role in (
        "destination_owner",
        "phase_owner",
        "synthetic_owner",
        "counted_owner",
    ):
        schema = found.recognized[0].schema.model_copy(
            update={
                "slots": (
                    first[0].model_copy(update={"role": role}),
                    *frame.leaf_schema.slots[1:4],
                )
            }
        )
        with pytest.raises(ValueError, match="CardNP"):
            Target(format=1, nodes=(valid,)).verify(schema, "zh-Hant", {})


def test_n1_opaque_modifier_keeps_flat_target(engine: Classifier) -> None:

    raw = "自分の墓場の他のフォロワー７枚を選ぶ。"
    found = derive(raw, engine)
    frame, binding = found.bind(0)
    node = CardNP(
        kind="NP",
        constructor="CardNP",
        args=CardArgs(
            kind=SlotRef(slot="leaf_2"),
            quantity=SlotRef(slot="leaf_3"),
            owner=SlotRef(slot="leaf_0"),
            zone=SlotRef(slot="leaf_1"),
        ),
    )
    with pytest.raises(ValueError, match="supported source operand"):
        found.verify(
            raw, engine, frame, binding, target=Target(format=1, nodes=(node,))
        )


def test_n1_fixed_inspection_omission_has_three_distinct_use_points(
    engine: Classifier,
) -> None:
    raw = "{ファンファーレ}手札７枚を捨てる:自分のデッキの上８枚を見る。その中から、コスト６以下のカード１枚を公開して手札に加えてよい。残りを好きな順にデッキの下に置く。"
    found = derive(raw, engine)
    assert found.recognized[0].issues == ()
    omitted = tuple(
        o for o in found.recognized[0].occurrences if o.source_presence == "omitted"
    )
    assert tuple(o.resolution_rule for o in omitted) == (
        "owner.actor_hand_cost.v1",
        "owner.self_hand_destination.v1",
        "owner.return_inspected_deck.v1",
    )
    assert roles(found)["destination_owner"] == ["self", "self"]
    assert "加えてよい。残りを好きな順に" in found.field.parts[0].canonical_source
    assert all(
        not s.required for s in found.recognized[0].schema.slots if not s.occurrences
    )


def test_n1_inline_description_does_not_block_later_destination(
    engine: Classifier,
) -> None:
    raw = "自分のデッキから「場かEXエリアの仮条件」のカード７枚を探し、手札に加える。"
    found = derive(raw, engine)
    assert roles(found)["source_zone"] == [("deck",)]
    assert roles(found)["destination_zone"] == [("hand",)]
    assert "「場かEXエリアの仮条件」" in found.field.parts[0].canonical_source


@pytest.mark.parametrize(
    ("spelling", "code"),
    [
        ("場", "battlefield"),
        ("手札", "hand"),
        ("デッキ", "deck"),
        ("エボルヴデッキ", "evolve_deck"),
        ("墓場", "graveyard"),
        ("EXエリア", "ex"),
        ("消滅領域", "banish"),
    ],
)
def test_n1_seven_zones_keep_source_and_destination_independent(
    engine: Classifier, spelling: str, code: str
) -> None:
    unit = "体" if code == "battlefield" else "枚"
    raw = f"自分の{spelling}からフォロワー７{unit}を相手の{spelling}に戻す。"
    found = derive(raw, engine)
    assert roles(found)["source_zone"] == [(code,)]
    assert roles(found)["destination_zone"] == [(code,)]
    assert roles(found)["source_owner"] == ["self"]
    assert roles(found)["destination_owner"] == ["opponent"]
    assert "から" in found.field.parts[0].canonical_source


@pytest.mark.parametrize(
    ("noun", "role", "value"),
    [
        (
            "{仮クラス}のフォロワー",
            "class_filter",
            VocabularyReference(kind="vocabulary", key=("class", "synthetic")),
        ),
        ("トークン・フォロワー", "token_filter", "token"),
        (
            "カード",
            "rule_term",
            GlossaryReference(kind="glossary", key="term:object.card"),
        ),
        (
            "スペル",
            "counted_kind",
            VocabularyReference(kind="vocabulary", key=("type", "spell")),
        ),
        (
            "アミュレット",
            "counted_kind",
            VocabularyReference(kind="vocabulary", key=("type", "amulet")),
        ),
    ],
)
def test_n1_body_filters_use_distinct_catalog_categories(
    engine: Classifier, noun: str, role: str, value: object
) -> None:
    zone, unit = ("EXエリア", "体") if role == "token_filter" else ("手札", "枚")
    found = derive("自分の" + zone + "の" + noun + "７" + unit + "を選ぶ。", engine)
    assert found.recognized[0].issues == ()
    assert roles(found)[role] == [value]
    assert "仮クラス" not in found.field.parts[0].canonical_source
    if role == "rule_term":
        assert "counted_kind" not in roles(found)
    if role == "token_filter":
        assert "trait" not in roles(found)


@pytest.mark.parametrize("remaining", ["残り", "選んだカード"])
def test_n1_remaining_inspected_cards_can_return_to_whole_deck(
    engine: Classifier, remaining: str
) -> None:
    raw = f"自分のデッキの上７枚を見る。{remaining}をデッキに置く。"
    found = derive(raw, engine)
    assert roles(found)["destination_zone"] == [("deck",)]
    assert ("destination_owner" in roles(found)) is (remaining == "残り")
    if remaining == "残り":
        assert roles(found)["destination_owner"] == ["self"]
        assert "N1-SRC07.return_inspected_deck.zone" in found.registry_rows[0]
