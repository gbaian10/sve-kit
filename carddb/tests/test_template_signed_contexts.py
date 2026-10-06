"""A signed magnitude cannot borrow a resource, damage direction or unverified ability."""

from dataclasses import replace

import pytest

from sve_carddb.snapshot.values import digest, integer, object_value, parse
from sve_carddb.template_parameter_rules.current import Rule, Rules, resolve
from sve_carddb.template_parameters.candidate_matching import recognize
from sve_carddb.template_parameters.inventory import Candidates
from sve_carddb.template_parameters.models import Schema, Slot
from sve_carddb.template_parameters.references import References
from sve_carddb.template_parameters.signed_contexts import SIGNED_CONTEXTS
from sve_carddb.template_sources.inventory import entry
from sve_carddb.template_sources.normalizer import VERSION, partition
from sve_carddb.template_translations.members import _members

from .test_template_explicit_rules import entry_ref
from .test_template_parameters import HASH, candidate
from .test_template_rule_candidates import matches

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
    return References(terms={"スタック": [("term:ability.stack", "ability", HASH)]})


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
        assert (row["proposed_role"], row["value"], row["raw_hash"]) == (
            identifier + "_magnitude",
            2,
            digest("２".encode()),
        )
        segments = row["context_segments"]
        assert isinstance(segments, list)
        context = "".join(
            signed[integer(object_value(s)["start"]) : integer(object_value(s)["end"])]
            for s in segments
        )
        assert sign + "２する" in context
        assert row["context_hash"] == digest(context.encode())
        if identifier == "stack_delta":
            assert (row["target_id"], row["target_hash"]) == (
                "term:ability.stack",
                HASH,
            )
        else:
            assert row["target_id"] is None
            assert row["target_hash"] is None
    assert c.model_dump() == original
    assert c.parameter_schema is None
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
def test_signed_current_resolution_keeps_role_bounds_and_ownership(
    identifier: str, text: str
) -> None:
    evidence = refs()
    c = candidate(text, evidence)
    ref = entry_ref()
    e = entry(ref, partition(text)[0], VERSION)
    c = c.model_copy(update={"inventory_id": e.id})
    rows = recognize(text, partition(text)[0], c, evidence, (identifier,))
    proposals = Candidates(entries=[c], rule_matches=list(rows))
    policy = Rules(
        parameter_rule_format=2,
        kind="template_parameter_rules",
        rules=(
            Rule(
                rule_id=identifier, enabled=True, origin="project", low_confidence=False
            ),
        ),
    )
    solved, pending = resolve(policy, proposals)
    assert pending == ()
    assert len(solved) == 1
    numeric = next(h for h in c.slots if h.type == "uint")
    m = _members(
        (e,),
        (c,),
        {(ref.source_version_id, ref.locator): text},
        {(e.id, numeric.name): object_value(parse(solved[0]))},
        {},
    )[0]
    assert m.pending == ()
    assert (
        next(r for h, r in zip(m.hints, m.roles, strict=True) if h.name == numeric.name)
        == identifier + "_magnitude"
    )
    slots = []
    for h in m.hints:
        assert h.type is not None
        slots.append(
            Slot(
                name=h.name,
                type=h.type,
                occurrences=(h.occurrence,),
                reference_kind=h.reference_kind,
                min=0 if h.type == "uint" else None,
                max=9007199254740991 if h.type == "uint" else None,
            )
        )
    schema = Schema(slots=tuple(slots))
    m.verify_schema(schema)
    wrong = replace(
        m,
        roles=tuple(
            "card_ordinal" if h.type == "uint" else r
            for h, r in zip(m.hints, m.roles, strict=True)
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Template numeric bounds differ from the recognized role$"
    ):
        wrong.verify_schema(schema)
    assert resolve(policy.model_copy(update={"rules": ()}), proposals)[0] == ()
    owned = c.model_copy(
        update={
            "slots": tuple(
                h.model_copy(update={"numeric_rule": "prefix_field_cost"})
                if h.type == "uint"
                else h
                for h in c.slots
            )
        }
    )
    assert recognize(text, partition(text)[0], owned, evidence, (identifier,)) == ()


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
        References(terms={"スタック": [("term:ability.combo", "ability", HASH)]}),
        References(terms={"スタック": [("term:ability.stack", "rule_term", HASH)]}),
        References(terms={"スタック": [("term:ability.stack", "ability", HASH)] * 2}),
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
