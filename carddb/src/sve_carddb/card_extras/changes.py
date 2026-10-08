"""Identifier-only conflict and downstream reports, without semantic adjudication."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal, cast

from pydantic import JsonValue

if TYPE_CHECKING:
    from collections.abc import Iterable

    from sve_carddb.build import Database
    from sve_carddb.card_extras.models import QAPage
    from sve_carddb.card_extras.plan import ExtrasPlan


def conflicts(pages: Iterable[QAPage]) -> list[dict[str, JsonValue]]:
    """Expose all conflicting observations in the caller's explicit generation scope."""
    groups: dict[str, dict[str, list[dict[str, JsonValue]]]] = defaultdict(
        lambda: defaultdict(list)
    )
    for page in pages:
        for block in page.blocks:
            entry = block.entry
            groups[entry.identity(page.region)][entry.fingerprint()].append(
                {
                    "source_id": page.source.id,
                    "locator": entry.locator,
                    "url": page.source.url,
                }
            )
    return [
        {
            "qa_id": owner,
            "observations": [
                item
                for fingerprint in sorted(variants)
                for item in variants[fingerprint]
            ],
        }
        for owner, variants in sorted(groups.items())
        if len(variants) > 1
    ]


@dataclass(frozen=True)
class DownstreamUse:
    qa_version_id: str
    kind: Literal["ruling", "dsl", "translation"]
    id: str


def changes(
    previous: Database,
    plan: ExtrasPlan,
    *,
    references: tuple[DownstreamUse, ...] | None = None,
) -> dict[str, JsonValue]:
    """List new observed versions and old downstream references needing inspection."""
    old = {
        str(row.values["id"]): str(row.values["qa_id"])
        for row in previous.rows("qa_version")
    }
    new = [version for version in plan.questions if version.id not in old]
    owners = {version.qa_id for version in new}
    old_cards: dict[str, set[str]] = defaultdict(set)
    for row in previous.rows("qa_card"):
        old_cards[str(row.values["qa_version_id"])].add(str(row.values["card_id"]))
    owners.update(
        version.qa_id
        for version in plan.questions
        if version.id in old and set(version.card_ids) != old_cards[version.id]
    )
    affected = {version for version, owner in old.items() if owner in owners}
    downstream: dict[str, JsonValue] = {}
    if references is not None and any(
        use.qa_version_id not in old for use in references
    ):
        raise ValueError(
            "Downstream inventory references an unknown previous QA version"
        )
    for kind in ("ruling", "dsl", "translation"):
        downstream[kind] = (
            None
            if references is None
            else _ids(
                sorted(
                    {
                        use.id
                        for use in references
                        if use.kind == kind and use.qa_version_id in affected
                    }
                )
            )
        )
    downstream["card_ids"] = _ids(
        sorted(
            {
                card
                for version in plan.questions
                if version.qa_id in owners
                for card in version.card_ids
            }
            | {
                str(row.values["card_id"])
                for row in previous.rows("qa_card")
                if row.values["qa_version_id"] in affected
            }
        )
    )
    return {
        "new_qa_versions": _ids(sorted(version.id for version in new)),
        "affected_downstream": downstream,
    }


def _ids(values: list[str]) -> list[JsonValue]:
    return [cast("JsonValue", value) for value in values]
