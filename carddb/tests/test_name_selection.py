"""Name policy selection never conveys owner or human-sample eligibility."""

from dataclasses import replace

import pytest

from sve_carddb.digital_name_policies.evaluate import NameOwner as PolicyOwner
from sve_carddb.digital_name_policies.evaluate import NamePolicyResult
from sve_carddb.snapshot.values import digest
from sve_carddb.translations.name_build import NameOwner, NameSource
from sve_carddb.translations.name_selection import (
    NameCandidate,
    first_counterpart,
    select_owner_name,
)

SOURCE = NameSource(
    NameOwner("face_revision", "synthetic-revision"),
    "c:" + "a" * 32,
    "f:" + "b" * 32,
    "unit",
    "ja",
    "Synthetic name",
    digest(b"Synthetic name"),
)
OWNER = PolicyOwner(
    kind="face_revision",
    owner_id=SOURCE.owner.identifier,
    card_id=SOURCE.card_id,
    face_id=SOURCE.face_id,
    printing_id="p:" + "c" * 32,
    state="unknown",
    name_ref=None,
)
POLICY = NamePolicyResult(
    OWNER,
    digest(b"context"),
    digest(b"policy"),
    SOURCE.source_hash,
    "untranslated",
    "nonunique_or_missing_translation",
    None,
    None,
    (),
    (),
)


def candidate(
    text: str, origin: str = "machine", *, human: bool = False
) -> NameCandidate:
    """Synthetic candidates represent already checked evidence, never adopted rows."""
    return NameCandidate(
        text,
        origin,
        "digital_official" if origin.startswith("official_") else "unofficial",
        "synthetic-decision",
        "2026-10-03T00:00:00Z",
        None,
        human_selected=human,
        counterpart_checked=origin.startswith("official_"),
    )


def chosen(
    *,
    policy: NamePolicyResult = POLICY,
    automatic: NameCandidate | None = None,
    choices: tuple[NameCandidate, ...] = (),
    counterparts: tuple[NameCandidate, ...] = (),
) -> str | None:
    result = select_owner_name(
        SOURCE,
        policy,
        context_hash=POLICY.context_hash,
        choices=choices,
        counterparts=counterparts,
        policy_candidate=automatic,
    )
    return None if result.candidate is None else result.candidate.text


def test_priority_actual_human_then_policy_then_own_counterpart_then_machine() -> None:
    automatic = candidate("規則譯名", "official_sv1")
    eligible = replace(POLICY, status="eligible", game="sv1", text=automatic.text)
    machine = candidate("機器譯名")
    human = candidate("親選譯名", human=True)
    b = candidate("同卡供名", "official_svwb")
    assert (
        chosen(
            policy=eligible,
            automatic=automatic,
            choices=(machine, human),
            counterparts=(b,),
        )
        == human.text
    )
    assert (
        chosen(
            policy=eligible, automatic=automatic, choices=(machine,), counterparts=(b,)
        )
        == automatic.text
    )
    assert chosen(choices=(machine,), counterparts=(b,)) == b.text
    assert chosen(choices=(machine,)) == machine.text
    assert chosen() is None


def test_sampled_nonmember_keeps_machine_origin_and_lower_priority() -> None:
    nonmember = candidate("未抽中的機器名")
    assert nonmember.origin == "machine"
    assert (
        chosen(
            choices=(nonmember,),
            counterparts=(candidate("自己的同卡譯名", "official_svwb"),),
        )
        == "自己的同卡譯名"
    )


def test_excluded_name_blocks_policy_fallback_but_preserves_actual_personal_choice() -> (
    None
):
    excluded = replace(POLICY, status="excluded", condition="name_exclusion")
    b = candidate("同卡供名", "official_svwb")
    assert chosen(policy=excluded, counterparts=(b,)) is None
    assert chosen(policy=excluded, choices=(b,), counterparts=(b,)) is None
    personal = replace(b, human_selected=True)
    assert chosen(policy=excluded, choices=(personal,), counterparts=(b,)) == b.text
    assert (
        chosen(policy=excluded, choices=(candidate("自譯"),), counterparts=(b,))
        == "自譯"
    )


def test_same_context_never_borrows_another_owner_policy_result() -> None:
    for source in (
        replace(SOURCE, card_id="c:" + "d" * 32),
        replace(SOURCE, face_id="f:" + "e" * 32),
        replace(SOURCE, owner=NameOwner("face_revision", "third")),
        replace(SOURCE, source_hash=digest(b"old printing")),
        replace(SOURCE, lang="en"),
    ):
        with pytest.raises(
            ValueError, match=r"^Name policy result differs from its exact build owner$"
        ):
            select_owner_name(
                source,
                POLICY,
                context_hash=POLICY.context_hash,
                choices=(),
                counterparts=(),
                policy_candidate=None,
            )


@pytest.mark.parametrize(
    "changed", ["presence", "text", "game", "authority", "human", "context"]
)
def test_checked_rule_result_cannot_be_substituted(changed: str) -> None:
    automatic = candidate("規則譯名", "official_sv1")
    value: NameCandidate | None = automatic
    policy = replace(POLICY, status="eligible", game="sv1", text="規則譯名")
    if changed == "presence":
        value = None
    elif changed == "text":
        value = replace(automatic, text="其他")
    elif changed == "game":
        value = replace(automatic, origin="official_svwb")
    elif changed == "authority":
        value = replace(automatic, authority="unofficial")
    elif changed == "human":
        value = replace(automatic, human_selected=True)
    else:
        policy = replace(policy, context_hash=digest(b"another build"))
    message = (
        "Name policy result differs from its exact build owner"
        if changed == "context"
        else "Name policy candidate differs from checked rule result"
    )
    with pytest.raises(ValueError, match=r"^" + message + "$"):
        chosen(policy=policy, automatic=value)


def test_both_resolvers_share_first_generation_priority_and_differences_are_reported() -> (
    None
):
    first, second = (
        candidate("一代名", "official_sv1"),
        candidate("二代名", "official_svwb"),
    )
    assert first_counterpart((second, first)) == first
    result = select_owner_name(
        SOURCE,
        POLICY,
        context_hash=POLICY.context_hash,
        choices=(),
        counterparts=(second, first),
        policy_candidate=None,
    )
    assert result.candidate == first
    assert result.differences == (digest(second.text.encode()),)


def test_human_or_same_game_conflict_refuses_one_exact_context() -> None:
    with pytest.raises(
        ValueError, match=r"^Exact name context has conflicting human selections$"
    ):
        chosen(choices=(candidate("甲", human=True), candidate("乙", human=True)))
    with pytest.raises(ValueError, match=r"^Ambiguous adopted digital names$"):
        chosen(
            counterparts=(
                candidate("甲", "official_sv1"),
                candidate("乙", "official_sv1"),
            )
        )


@pytest.mark.parametrize(
    "value", [candidate(""), replace(candidate("甲"), authority="digital_official")]
)
def test_invalid_origin_authority_is_refused(value: NameCandidate) -> None:
    with pytest.raises(
        ValueError, match=r"^Name candidate origin and authority are inconsistent$"
    ):
        chosen(choices=(value,))
