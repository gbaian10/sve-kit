"""ID sets, precise event triples and historical bridges are distinct proof obligations."""

import copy
import re

import pytest

from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.template_parameter_rules.events import (
    verify_event_history,
    verify_events,
)
from sve_carddb.template_parameter_rules.loader import parse_approval, parse_policy

from .recognition_policy_fixtures import pair

MATCHER = "a" * 40


def test_legal_bulk_event_does_not_need_clicks_or_samples() -> None:
    policy, receipt = pair(MATCHER)
    verify_events(parse_policy(canonical(policy)), parse_approval(canonical(receipt)))


def test_eight_old_ids_and_seventeen_new_authorizations_are_separate() -> None:
    policy, receipt = pair(MATCHER, bridge=True)
    verify_events(parse_policy(canonical(policy)), parse_approval(canonical(receipt)))


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("extra_row", "Recognition approval must cover exactly the policy rules"),
        ("row_hash", "Recognition approval row differs from the policy triple"),
        ("row_commit", "Recognition approval row differs from the policy triple"),
        (
            "missing_event",
            "Recognition row must reference existing authorization and presentation events",
        ),
        (
            "missing_presentation_event",
            "Recognition row must reference existing authorization and presentation events",
        ),
        ("missing_authorization", "Recognition row lacks its original authorization"),
        (
            "missing_presentation",
            "Recognition current rule was not presented in the referenced event",
        ),
        (
            "extra_authorization",
            "Recognition event authorization IDs must equal its referencing rows",
        ),
        (
            "event_hash",
            "Recognition current rule was not presented in the referenced event",
        ),
        (
            "other_event_presentation",
            "Recognition hashed authorization cannot use a historical bridge",
        ),
        (
            "unexpected_restriction",
            "Recognition hashed authorization cannot use a historical bridge",
        ),
        ("unused_event", "Recognition receipt cannot retain unused events"),
        ("uuid_alias", "Recognition events cannot reuse a message identifier"),
        (
            "private_locator",
            "Recognition event locator must be a durable public identifier",
        ),
        (
            "private_source_locator",
            "Recognition event locator must be a durable public identifier",
        ),
    ],
)
def test_precise_event_refusals(  # ruff: ignore[complex-structure,too-many-branches] -- each variant preserves all evidence except its targeted event check
    mutation: str, message: str
) -> None:
    policy, receipt = pair(MATCHER)
    rows = array(receipt["rules"])
    row = object_value(rows[0])
    events = object_value(receipt["events"])
    event = object_value(events["event_20261002_2"])
    presentation = object_value(event["presentation"])
    if mutation == "extra_row":
        policy["rules"] = array(policy["rules"])[1:]
    elif mutation in {"row_hash", "row_commit"}:
        row["condition_hash" if mutation == "row_hash" else "matcher_commit"] = (
            digest(b"different") if mutation == "row_hash" else "b" * 40
        )
    elif mutation in {"missing_event", "missing_presentation_event"}:
        row["event_id" if mutation == "missing_event" else "presented_in"] = (
            "missing_event"
        )
    elif mutation == "missing_authorization":
        event["authorized_rules"] = array(event["authorized_rules"])[1:]
    elif mutation == "missing_presentation":
        extra = copy.deepcopy(event)
        events["event_20261002_3"] = extra
        object_value(extra["authorization_basis"])["event_locator"] = (
            "decision:synthetic/3"
        )
        row["presented_in"] = "event_20261002_3"
        object_value(extra["presentation"])["presented_rules"] = array(
            presentation["presented_rules"]
        )[1:]
        extra["authorized_rules"] = array(event["authorized_rules"])[1:]
    elif mutation == "extra_authorization":
        receipt["rules"] = rows[1:]
        policy["rules"] = array(policy["rules"])[1:]
    elif mutation == "event_hash":
        for items in (
            array(event["authorized_rules"]),
            array(presentation["presented_rules"]),
        ):
            object_value(items[0])["condition_hash"] = digest(b"changed")
    elif mutation in {"other_event_presentation", "uuid_alias", "unused_event"}:
        other = copy.deepcopy(event)
        events["event_20261002_3"] = other
        if mutation != "uuid_alias":
            object_value(other["authorization_basis"])["event_locator"] = (
                "decision:synthetic/3"
            )
        if mutation == "other_event_presentation":
            row["presented_in"] = "event_20261002_3"
    elif mutation == "unexpected_restriction":
        row["restriction_ids"] = ["arbitrary_notice"]
    else:
        object_value(event["authorization_basis"])[
            "event_locator" if mutation == "private_locator" else "source_locator"
        ] = "/private/path/evidence"
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        verify_events(
            parse_policy(canonical(policy)), parse_approval(canonical(receipt))
        )


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        (
            "no_restriction",
            "Recognition old event requires the single explicit sign restriction bridge",
        ),
        (
            "other_restriction",
            "Recognition old event requires the single explicit sign restriction bridge",
        ),
        (
            "same_event",
            "Recognition current rule was not presented in the referenced event",
        ),
        ("earlier", "Recognition bridge presentation cannot precede authorization"),
        (
            "no_disclosure",
            "Recognition bridge requires its presented restriction evidence",
        ),
        ("no_row_note", "Recognition historical bridge must retain disclosure notes"),
        ("lost_old_id", "Recognition row lacks its original authorization"),
        (
            "new_authorizes_old",
            "Recognition event authorization IDs must equal its referencing rows",
        ),
    ],
)
def test_historical_bridge_refusals(mutation: str, message: str) -> None:
    policy, receipt = pair(MATCHER, bridge=True)
    rows = array(receipt["rules"])
    row = next(
        object_value(r)
        for r in rows
        if object_value(r)["event_id"] == "event_20261002_1"
    )
    events = object_value(receipt["events"])
    new = object_value(events["event_20261002_2"])
    if mutation == "no_restriction":
        row["restriction_ids"] = []
    elif mutation == "other_restriction":
        row["restriction_ids"] = ["other_notice"]
    elif mutation == "same_event":
        row["presented_in"] = "event_20261002_1"
    elif mutation == "earlier":
        new["reviewed_at"] = "2026-10-01T21:03:25.150Z"
    elif mutation == "no_disclosure":
        object_value(new["presentation"])["disclosures"] = []
    elif mutation == "no_row_note":
        row["note"] = ""
    elif mutation == "lost_old_id":
        # Keep the old identity set valid; remove its referencing row instead.
        receipt["rules"] = [
            r for r in rows if object_value(r)["rule_id"] != row["rule_id"]
        ]
        policy["rules"] = [
            r
            for r in array(policy["rules"])
            if object_value(r)["rule_id"] != row["rule_id"]
        ]
        message = "Recognition event authorization IDs must equal its referencing rows"
    else:
        new["authorized_rules"] = object_value(new["presentation"])["presented_rules"]
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        verify_events(
            parse_policy(canonical(policy)), parse_approval(canonical(receipt))
        )


@pytest.mark.parametrize(
    "mutation",
    [
        "half_hash",
        "half_commit",
        "version",
        "missing_version",
        "page_hash",
        "sample",
        "day",
        "empty_authorization",
        "clicks",
    ],
)
def test_closed_event_identity_refusals(mutation: str) -> None:
    _, receipt = pair(MATCHER, bridge=True)
    events = object_value(receipt["events"])
    old = object_value(events["event_20261002_1"])
    identity = object_value(array(old["authorized_rules"])[0])
    if mutation == "half_hash":
        identity["condition_hash"] = digest(b"new")
    elif mutation == "half_commit":
        identity["matcher_commit"] = MATCHER
    elif mutation == "version":
        identity["matcher_version"] = "numeric-rule-proposals-v3:" + str(
            identity["rule_id"]
        )
    elif mutation == "missing_version":
        identity.pop("matcher_version")
    elif mutation == "page_hash":
        object_value(old["presentation"])["page_hash"] = None
    elif mutation == "sample":
        old["sample_ids"] = ["fabricated_sample"]
    elif mutation == "day":
        old["reviewed_precision"] = "day"
    elif mutation == "clicks":
        old["form"] = "page_rule"
    else:
        old["authorized_rules"] = []
    with pytest.raises(ValueError, match=r"^Invalid recognition approval envelope$"):
        parse_approval(canonical(receipt))


@pytest.mark.parametrize(
    "mutation", ["scope", "reviewer", "time", "statement", "form", "triple"]
)
def test_actual_event_cannot_be_rewritten_in_a_new_pair(mutation: str) -> None:
    _, first = pair(MATCHER)
    second = copy.deepcopy(first)
    event = object_value(object_value(second["events"])["event_20261002_2"])
    if mutation == "scope":
        event["authorized_rules"] = array(event["authorized_rules"])[1:]
    elif mutation == "triple":
        for items in (
            array(event["authorized_rules"]),
            array(object_value(event["presentation"])["presented_rules"]),
        ):
            object_value(items[0])["matcher_commit"] = "b" * 40
    else:
        key, value = {
            "reviewer": ("reviewed_by", "Synthetic Other"),
            "time": ("reviewed_at", "2026-10-03T00:00:00Z"),
            "statement": ("note", "Claims fabricated per-example review"),
            "form": ("form", "page_bulk"),
        }[mutation]
        event[key] = value
    with pytest.raises(
        ValueError,
        match=r"^Recognition immutable event changed across policy versions$",
    ):
        verify_event_history(
            (parse_approval(canonical(first)), parse_approval(canonical(second)))
        )


def test_new_actual_event_can_authorize_a_reduced_set() -> None:
    policy, original = pair(MATCHER)
    newer = copy.deepcopy(original)
    policy["rules"] = array(policy["rules"])[1:]
    newer["rules"] = array(newer["rules"])[1:]
    event = object_value(object_value(newer["events"])["event_20261002_2"])
    event["authorized_rules"] = array(event["authorized_rules"])[1:]
    object_value(event["presentation"])["presented_rules"] = array(
        object_value(event["presentation"])["presented_rules"]
    )[1:]
    object_value(event["authorization_basis"])["event_locator"] = (
        "decision:synthetic/new_explicit_scope"
    )
    verify_events(parse_policy(canonical(policy)), parse_approval(canonical(newer)))
    verify_event_history(
        (parse_approval(canonical(original)), parse_approval(canonical(newer)))
    )


def test_old_hash_cannot_be_borrowed_from_another_presentation() -> None:
    policy, receipt = pair(MATCHER)
    events = object_value(receipt["events"])
    origin = object_value(events["event_20261002_2"])
    later = copy.deepcopy(origin)
    object_value(later["authorization_basis"])["event_locator"] = (
        "decision:synthetic/later"
    )
    events["event_20261002_3"] = later
    for items in (
        array(origin["authorized_rules"]),
        array(object_value(origin["presentation"])["presented_rules"]),
    ):
        object_value(items[0])["condition_hash"] = digest(
            b"earlier different conditions"
        )
    object_value(array(receipt["rules"])[0])["presented_in"] = "event_20261002_3"
    with pytest.raises(
        ValueError,
        match=r"^Recognition hashed authorization differs from the current matcher$",
    ):
        verify_events(
            parse_policy(canonical(policy)), parse_approval(canonical(receipt))
        )


def test_scope_change_requires_its_own_actual_event() -> None:
    policy, first = pair(MATCHER)
    other = copy.deepcopy(policy)
    other["policy_id"] = "synthetic-v2"
    object_value(other["scope"])["roles"] = ["body"]
    second = copy.deepcopy(first)
    second["policy_id"] = "synthetic-v2"
    with pytest.raises(
        ValueError,
        match=r"^Recognition reused event cannot authorize a different policy scope$",
    ):
        verify_event_history(
            (parse_approval(canonical(first)), parse_approval(canonical(second))),
            (parse_policy(canonical(policy)), parse_policy(canonical(other))),
        )
