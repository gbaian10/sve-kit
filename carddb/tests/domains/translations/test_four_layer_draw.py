"""Closed draw rules require the complete source and exact face context."""

import hashlib
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import PrintingFaceOwner
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from .test_four_layer_classification import classifier, source

if TYPE_CHECKING:
    from sve_carddb.contracts.source_binding import SourceDescriptor


def _ascii_id(value: object) -> str | None:
    # Keep source wording out of test IDs and reports.
    if isinstance(value, str) and not value.isascii():
        return "text-" + hashlib.sha256(value.encode()).hexdigest()[:8]
    return None


@pytest.mark.parametrize(
    ("head", "kind", "phase", "key"),
    [
        ("", "spell", "normal", "draw_exact_spell.v1"),
        ("【攻撃時】", "follower", "normal", "draw_exact_attack_time.v1"),
        ("【進化時】", "follower", "evolved", "draw_exact_evolve_time.v1"),
    ],
    ids=_ascii_id,
)
@pytest.mark.parametrize("count", [0, 1, 3, 9007199254740991])
def test_draw_replays_exact_source_and_fixed_scope(
    head: str, kind: str, phase: str, key: str, count: int
) -> None:
    raw = f"{head}{count}枚引く。"
    facts = CardContext(source(raw), "c:synthetic", "f:synthetic", phase, kind, True, 0)
    engine = classifier()
    derived = derive_source(raw, facts.source, engine, context=facts)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding, context=facts)
    assert frame.semantic_variant.key == key
    assert frame.projection.projection_kind == "ability_body"
    assert [scope.id for scope in frame.projection.scopes] == ["ability_0"]
    assert frame.projection.imports == frame.projection.exports == ()
    assert binding.values == {"leaf_0": count}


@pytest.mark.parametrize(
    "raw",
    [
        "1枚まで引く。",
        "相手は1枚引く。",
        "お互いに1枚引く。",
        "1枚引いてよい。",
        "1枚引く。1枚捨てる。",
        "条件なら、1枚引く。",
        "{クイック}\n1枚引く。",
        "追加コストを払う。\n1枚引く。",
        "1枚引く。\n仮。",
        "1枚引く。（追加の条件）",
        "01枚引く。",
        "１枚引く。",
        " 1枚引く。",
        "【ファンファーレ】1枚引く。",
        "【超進化時】1枚引く。",
    ],
    ids=_ascii_id,
)
def test_other_complete_constructions_do_not_gain_draw_semantics(raw: str) -> None:
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", "normal", "spell", True, 0
    )
    derived = derive_source(raw, facts.source, classifier(), context=facts)
    assert all(
        found.semantics is None or not found.semantics.key.startswith("draw_exact")
        for found in derived.recognized
    )


@pytest.mark.parametrize(
    "mutation",
    [
        "history",
        "sections",
        "unknown_sections",
        "kind",
        "phase",
        "field",
        "printing",
        "missing",
    ],
)
def test_spell_requires_known_current_complete_context(mutation: str) -> None:
    raw = "1枚引く。"
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", "normal", "spell", True, 0
    )
    if mutation == "history":
        facts = replace(facts, current=False)
    elif mutation == "sections":
        facts = replace(facts, section_count=1)
    elif mutation == "unknown_sections":
        facts = replace(facts, section_count=None)
    elif mutation == "kind":
        facts = replace(facts, card_kind="follower")
    elif mutation == "phase":
        facts = replace(facts, phase="evolved")
    elif mutation == "field":
        facts = replace(facts, source=source(raw, "section"))
    elif mutation == "printing":
        facts = replace(facts, source=_printed(facts))
    derived = derive_source(
        raw,
        facts.source,
        classifier(),
        context=None if mutation == "missing" else facts,
    )
    assert derived.recognized[0].semantics is None


def _printed(facts: CardContext) -> SourceDescriptor:
    owner = PrintingFaceOwner(
        kind="printing_face", printing_id="p:synthetic", face_id=facts.face_id
    )
    return facts.source.model_copy(update={"owner": owner})


@pytest.mark.parametrize(
    ("head", "phase"),
    [("【攻撃時】", "normal"), ("【進化時】", "evolved")],
    ids=_ascii_id,
)
@pytest.mark.parametrize(
    "mutation", ["history", "spell", "amulet", "field", "printing", "missing"]
)
def test_trigger_requires_current_follower_effect_owner(
    head: str, phase: str, mutation: str
) -> None:
    raw = head + "1枚引く。"
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", phase, "follower", True, 0
    )
    if mutation == "history":
        facts = replace(facts, current=False)
    elif mutation in {"spell", "amulet"}:
        facts = replace(facts, card_kind=mutation)
    elif mutation == "field":
        facts = replace(facts, source=source(raw, "section"))
    elif mutation == "printing":
        facts = replace(facts, source=_printed(facts))
    derived = derive_source(
        raw,
        facts.source,
        classifier(),
        context=None if mutation == "missing" else facts,
    )
    assert derived.recognized[0].semantics is None


@pytest.mark.parametrize("phase", ["normal", "advance"])
def test_evolve_time_requires_a_printed_evolve_face(phase: str) -> None:
    raw = "【進化時】1枚引く。"
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", phase, "follower", True, 0
    )
    derived = derive_source(raw, facts.source, classifier(), context=facts)
    assert derived.recognized[0].semantics is None


def test_trigger_keeps_siblings_without_claiming_whole_card() -> None:
    raw = "【進化時】1枚引く。\n【超進化時】1枚引く。"
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", "evolved", "follower", True, 0
    )
    derived = derive_source(raw, facts.source, classifier(), context=facts)
    assert derived.bind(0)[0].semantic_variant.key == "draw_exact_evolve_time.v1"
    assert derived.recognized[-1].semantics is None


@pytest.mark.parametrize("head", ["【攻撃時】", "【進化時】"], ids=_ascii_id)
@pytest.mark.parametrize(
    "body",
    [
        "1枚まで引く。",
        "相手は1枚引く。",
        "1枚引いてよい。",
        "手札1枚を捨てる：1枚引く。",
        "条件なら、1枚引く。",
        "1枚引く。1枚捨てる。",
    ],
    ids=_ascii_id,
)
def test_trigger_does_not_drop_control_or_actor(head: str, body: str) -> None:
    raw = head + body
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", "evolved", "follower", True, 0
    )
    derived = derive_source(raw, facts.source, classifier(), context=facts)
    assert derived.recognized[0].semantics is None


@pytest.mark.parametrize("head", ["【攻撃時】", "【進化時】"], ids=_ascii_id)
@pytest.mark.parametrize("quick", ["{クイック}", "｛クイック｝"], ids=_ascii_id)
def test_quick_prefix_does_not_authorize_an_independent_trigger(
    head: str, quick: str
) -> None:
    raw = quick + "\n" + head + "1枚引く。"
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", "evolved", "follower", True, 0
    )
    derived = derive_source(raw, facts.source, classifier(), context=facts)
    assert derived.recognized[-1].semantics is None


@pytest.mark.parametrize("head", ["【攻撃時】", "【進化時】"], ids=_ascii_id)
@pytest.mark.parametrize(
    ("before", "after"),
    [
        ("「仮\n", "\n仮」"),
        ("（仮\n", "\n仮）"),
        ("『仮\n", "\n仮』"),
        ("仮「仮\n", "\n」を得る。"),
        ("「仮\n", ""),
        ("", "\n仮」"),
    ],
    ids=_ascii_id,
)
def test_line_inside_cross_line_brackets_is_not_an_ability(
    head: str, before: str, after: str
) -> None:
    raw = before + head + "1枚引く。" + after
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", "evolved", "follower", True, 0
    )
    derived = derive_source(raw, facts.source, classifier(), context=facts)
    assert all(
        found.semantics is None or not found.semantics.key.startswith("draw_exact")
        for found in derived.recognized
    )


@pytest.mark.parametrize("sibling", ["「仮」", "（仮）"], ids=_ascii_id)
def test_closed_sibling_brackets_keep_the_independent_trigger(sibling: str) -> None:
    raw = sibling + "\n【進化時】1枚引く。"
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", "evolved", "follower", True, 0
    )
    derived = derive_source(raw, facts.source, classifier(), context=facts)
    assert derived.bind(len(derived.recognized) - 1)[0].semantic_variant.key == (
        "draw_exact_evolve_time.v1"
    )
