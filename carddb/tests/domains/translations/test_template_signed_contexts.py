"""A signed magnitude cannot borrow a resource, damage direction or unverified ability."""

import pytest

from sve_carddb.core.json import integer, object_value
from sve_carddb.domains.translations.recognition.candidate_matching import recognize
from sve_carddb.domains.translations.recognition.references import References
from sve_carddb.domains.translations.recognition.signed_contexts import SIGNED_CONTEXTS

from .test_template_parameters import candidate, matches, partition

CASES = (
    ("pp_capacity_delta", "自分のPP最大値を+２する。"),
    ("pp_delta", "自分のPPを+２する。"),
    ("ep_delta", "自分のEPを+２する。"),
    ("stack_delta", "自分の場の【スタック】を+２する。"),
    *(
        (f"{direction}_{kind}_damage_delta", f"これが{verb}{noun}ダメージを+２する。")
        for direction, verb in (("dealt", "与える"), ("received", "受ける"))
        for kind, noun in (("all", ""), ("ability", "能力"), ("combat", "交戦"))
    ),
)


def refs() -> References:
    return References(terms={"スタック": [("term:ability.stack", "ability")]})


@pytest.mark.parametrize(("identifier", "text"), CASES)
def test_signed_roles_require_opt_in_and_raw_magnitude_keep_literal_sign(
    identifier: str, text: str
) -> None:
    evidence = refs()
    c = candidate(text, evidence)
    original = c.model_dump()
    assert recognize(text, partition(text)[0], c, evidence) == ()
    for sign in ("+", "-"):
        signed = text.replace("+", sign)
        rows = matches(signed, identifier, evidence)
        assert len(rows) == 1
        row = rows[0]
        assert (row["recognized_role"], row["value"]) == (
            identifier + "_magnitude",
            2,
        )
        segments = row["context_segments"]
        assert isinstance(segments, list)
        context = "".join(
            signed[integer(object_value(s)["start"]) : integer(object_value(s)["end"])]
            for s in segments
        )
        assert sign + "２する" in context
        if identifier == "stack_delta":
            assert row["target_id"] == "term:ability.stack"
        else:
            assert row["target_id"] is None
    assert c.model_dump() == original
    assert matches(text.replace("２", "０"), identifier, evidence)
    assert matches(text.replace("２", "9007199254740991"), identifier, evidence)
    for raw in ("②", "9007199254740992", "2X", "２猫"):
        assert matches(text.replace("２", raw), identifier, evidence) == ()
    for sign in ("", "++", "--", "−"):
        assert matches(text.replace("+", sign), identifier, evidence) == ()
    assert matches(text.replace("する。", "する名。"), identifier, evidence) == ()
    assert matches(text.replace("する。", "読む。"), identifier, evidence) == ()
    assert matches("『" + text + "』", identifier, evidence) == ()
    hint = next(h for h in c.slots if h.type == "uint")
    damaged = c.model_copy(
        update={
            "slots": tuple(
                h.model_copy(update={"value": 8}) if h.name == hint.name else h
                for h in c.slots
            )
        }
    )
    assert recognize(text, partition(text)[0], damaged, evidence, (identifier,)) == ()


@pytest.mark.parametrize(("identifier", "text"), CASES)
def test_signed_contexts_cannot_borrow_another_resource_direction_or_type(
    identifier: str, text: str
) -> None:
    del text
    for other, other_text in CASES:
        if identifier != other:
            assert matches(other_text, identifier, refs()) == ()
    assert set(SIGNED_CONTEXTS) == {identifier for identifier, _ in CASES}


@pytest.mark.parametrize(
    ("identifier", "text"),
    [
        ("pp_delta", "自分のSEPを+２する。"),
        ("ep_delta", "自分のSEPを+２する。"),
        ("pp_capacity_delta", "超PP最大値を+２する。"),
        ("pp_capacity_delta", "相手のPP最大値を+２する。"),
        ("pp_capacity_delta", "自分のPP最大値が+２する。"),
        ("stack_delta", "自分の場の【未知】を+２する。"),
        ("stack_delta", "相手の場の【スタック】を+２する。"),
        ("stack_delta", "自分の場の【超スタック】を+２する。"),
        ("dealt_all_damage_delta", "これが受けるダメージを+２する。"),
        ("received_all_damage_delta", "これが受ける能力ダメージを+２する。"),
        ("received_all_damage_delta", "受けるダメージを+２する。"),
    ],
)
def test_nearby_incomplete_contexts_stay_pending(identifier: str, text: str) -> None:
    assert matches(text, identifier, refs()) == ()


@pytest.mark.parametrize(
    "evidence",
    [
        References(),
        References(terms={"スタック": [("term:ability.combo", "ability")]}),
        References(terms={"スタック": [("term:ability.stack", "rule_term")]}),
        References(terms={"スタック": [("term:ability.stack", "ability")] * 2}),
    ],
)
def test_stack_requires_unique_exact_adopted_ability(evidence: References) -> None:
    assert matches("自分の場の【スタック】を+２する。", "stack_delta", evidence) == ()


def test_observed_unqualified_capacity_payment_prohibition_and_granted_damage() -> None:
    assert matches("{試験}PP最大値を-２:試す。", "pp_capacity_delta")
    assert matches("次のスタートフェイズにPP最大値を+２できない。", "pp_capacity_delta")
    assert matches(
        "これは「これが次に受ける能力ダメージを-２する」を持つ。",
        "received_ability_damage_delta",
    )
