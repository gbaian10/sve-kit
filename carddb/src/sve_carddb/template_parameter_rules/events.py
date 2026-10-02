"""Compare actual authorization sets separately from later presentation and restrictions."""

import re
from datetime import datetime
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical
from sve_carddb.template_parameter_rules.models import LEGACY_IDS, RESTRICTION

if TYPE_CHECKING:
    from sve_carddb.template_parameter_rules.models import (
        Answer,
        Approval,
        Event,
        Identity,
        Policy,
        Rule,
    )

LOCATOR = re.compile(
    r"^(?:[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}|https://[^\s]+|decision:[a-z0-9][a-z0-9._:/#-]*)$"
)


def identity(rule: Identity | Rule) -> bytes:
    """Comparison includes the version without borrowing metadata for old events."""
    return canonical(
        {
            key: rule.model_dump(mode="json")[key]
            for key in (
                "rule_id",
                "matcher_version",
                "condition_hash",
                "matcher_commit",
            )
        }
    )


def verify_events(policy: Policy, receipt: Approval) -> None:
    """No presentation, notice or sample is itself a fresh authorization event."""
    rules = {item.rule_id: item for item in policy.rules}
    if receipt.policy_id != policy.policy_id or set(rules) != {
        a.rule_id for a in receipt.rules
    }:
        raise ValueError("Recognition approval must cover exactly the policy rules")
    used: set[str] = set()
    _locators(tuple(receipt.events.values()))
    for answer in receipt.rules:
        _answer(rules[answer.rule_id], answer, receipt)
        used.update((answer.event_id, answer.presented_in))
    if used != set(receipt.events):
        raise ValueError("Recognition receipt cannot retain unused events")
    for key, event in receipt.events.items():
        actual = {row.rule_id for row in receipt.rules if row.event_id == key}
        if actual != {item.rule_id for item in event.authorized_rules}:
            raise ValueError(
                "Recognition event authorization IDs must equal its referencing rows"
            )


def _answer(rule: Rule, answer: Answer, receipt: Approval) -> None:
    if (answer.condition_hash, answer.matcher_commit) != (
        rule.condition_hash,
        rule.matcher_commit,
    ):
        raise ValueError("Recognition approval row differs from the policy triple")
    if (
        answer.event_id not in receipt.events
        or answer.presented_in not in receipt.events
    ):
        raise ValueError(
            "Recognition row must reference existing authorization and presentation events"
        )
    origin = receipt.events[answer.event_id]
    presented = receipt.events[answer.presented_in]
    authorized = next(
        (i for i in origin.authorized_rules if i.rule_id == rule.rule_id), None
    )
    if authorized is None:
        raise ValueError("Recognition row lacks its original authorization")
    if not any(
        identity(i) == identity(rule) for i in presented.presentation.presented_rules
    ):
        raise ValueError(
            "Recognition current rule was not presented in the referenced event"
        )
    if authorized.condition_hash is not None:
        if identity(authorized) != identity(rule):
            raise ValueError(
                "Recognition hashed authorization differs from the current matcher"
            )
        if answer.event_id != answer.presented_in or answer.restriction_ids:
            raise ValueError(
                "Recognition hashed authorization cannot use a historical bridge"
            )
    else:
        _bridge(
            origin,
            presented,
            answer.event_id == answer.presented_in,
            answer.restriction_ids,
        )
        if not answer.note or not receipt.note:
            raise ValueError(
                "Recognition historical bridge must retain disclosure notes"
            )


def _locators(events: tuple[Event, ...]) -> None:
    """UUID aliases cannot fabricate independent authorizations."""
    locators: set[str] = set()
    for event in events:
        for locator in (
            event.authorization_basis.event_locator,
            event.authorization_basis.source_locator,
        ):
            if locator is not None and LOCATOR.fullmatch(locator) is None:
                raise ValueError(
                    "Recognition event locator must be a durable public identifier"
                )
        aliases = {event.authorization_basis.event_locator}
        if event.authorization_basis.source_locator is not None:
            aliases.add(event.authorization_basis.source_locator)
        if locators.intersection(aliases):
            raise ValueError("Recognition events cannot reuse a message identifier")
        locators.update(aliases)


def _bridge(
    origin: Event, presented: Event, same: bool, restrictions: tuple[str, ...]
) -> None:
    if same or restrictions != (RESTRICTION,):
        raise ValueError(
            "Recognition old event requires the single explicit sign restriction bridge"
        )
    if tuple(i.rule_id for i in origin.authorized_rules) != LEGACY_IDS:
        raise ValueError("Recognition bridge is restricted to the original eight rules")
    if datetime.fromisoformat(presented.reviewed_at) < datetime.fromisoformat(
        origin.reviewed_at
    ):
        raise ValueError("Recognition bridge presentation cannot precede authorization")
    if not any(item.id == RESTRICTION for item in presented.presentation.disclosures):
        raise ValueError(
            "Recognition bridge requires its presented restriction evidence"
        )


def verify_event_history(
    receipts: tuple[Approval, ...], policies: tuple[Policy, ...] = ()
) -> None:
    """An immutable message cannot authorize a different scope in a subsequent pair."""
    seen: dict[str, bytes] = {}
    scopes: dict[str, bytes] = {}
    by_policy = {policy.policy_id: policy for policy in policies}
    for receipt in receipts:
        for event in receipt.events.values():
            encoded = canonical(event.model_dump(mode="json"))
            for locator in (
                event.authorization_basis.event_locator,
                event.authorization_basis.source_locator,
            ):
                if locator is None:
                    continue
                if locator in seen and seen[locator] != encoded:
                    raise ValueError(
                        "Recognition immutable event changed across policy versions"
                    )
                seen[locator] = encoded
                if receipt.policy_id in by_policy:
                    scope = canonical(
                        by_policy[receipt.policy_id].scope.model_dump(mode="json")
                    )
                    if locator in scopes and scopes[locator] != scope:
                        raise ValueError(
                            "Recognition reused event cannot authorize a different policy scope"
                        )
                    scopes[locator] = scope
