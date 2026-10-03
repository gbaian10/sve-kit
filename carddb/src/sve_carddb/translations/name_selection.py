"""One owner-local ordering for policy names, actual human choices and counterparts."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.snapshot.values import digest

if TYPE_CHECKING:
    from sve_carddb.build_inputs import Source
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.digital_name_policies.evaluate import NamePolicyResult
    from sve_carddb.translations.name_build import NameSource

GAME_PRIORITY = ("sv1", "svwb")


@dataclass(frozen=True)
class NameCandidate:
    text: str
    origin: str
    authority: str
    decision_id: str | None
    reviewed_at: str
    source: Source | None
    refs: tuple[SourceRef, ...] = ()
    human_selected: bool = False
    choice_hash: str | None = None
    counterpart_checked: bool = False


@dataclass(frozen=True)
class NameSelection:
    candidate: NameCandidate | None
    reason: Literal["human", "policy", "counterpart", "choice", "untranslated"]
    differences: tuple[str, ...]


def _policy_owner(
    source: NameSource,
    policy: NamePolicyResult,
    *,
    context_hash: str,
    policy_candidate: NameCandidate | None,
) -> None:
    """Policy results remain tied to a build and exact owner even on a context hit."""
    owner = policy.owner
    if (
        policy.context_hash,
        owner.kind,
        owner.owner_id,
        owner.card_id,
        owner.face_id,
        policy.source_name_hash,
        "ja",
    ) != (
        context_hash,
        source.owner.kind,
        source.owner.identifier,
        source.card_id,
        source.face_id,
        source.source_hash,
        source.lang,
    ):
        raise ValueError("Name policy result differs from its exact build owner")
    if (policy.status == "eligible") != (policy_candidate is not None):
        raise ValueError("Name policy candidate differs from checked rule result")
    if policy_candidate is not None and (
        (
            policy_candidate.text,
            policy_candidate.origin,
            policy_candidate.authority,
            policy_candidate.human_selected,
        )
        != (policy.text, "official_" + str(policy.game), "digital_official", False)
    ):
        raise ValueError("Name policy candidate differs from checked rule result")


def first_counterpart(candidates: tuple[NameCandidate, ...]) -> NameCandidate | None:
    """Both policy application and the legacy same-card selector use sv1 first."""
    for game in GAME_PRIORITY:
        matches = tuple(c for c in candidates if c.origin == "official_" + game)
        if len({candidate.text for candidate in matches}) > 1:
            raise ValueError("Ambiguous adopted digital names")
        if matches:
            return min(
                matches, key=lambda c: (c.text, c.decision_id or "", c.reviewed_at)
            )
    return None


def select_owner_name(
    source: NameSource,
    policy: NamePolicyResult,
    *,
    context_hash: str,
    choices: tuple[NameCandidate, ...],
    counterparts: tuple[NameCandidate, ...],
    policy_candidate: NameCandidate | None,
) -> NameSelection:
    """Candidates already carry owner evidence; a shared context never grants it."""
    _policy_owner(
        source, policy, context_hash=context_hash, policy_candidate=policy_candidate
    )
    if any(
        not candidate.text
        or candidate.origin
        not in {"machine", "project", "community", "official_sv1", "official_svwb"}
        or candidate.authority
        != (
            "digital_official"
            if candidate.origin.startswith("official_")
            else "unofficial"
        )
        for candidate in (*choices, *counterparts)
    ):
        raise ValueError("Name candidate origin and authority are inconsistent")
    human = tuple(candidate for candidate in choices if candidate.human_selected)
    chosen: NameCandidate | None = None
    reason: Literal["human", "policy", "counterpart", "choice", "untranslated"] = (
        "untranslated"
    )
    if human:
        if len({candidate.text for candidate in human}) != 1:
            raise ValueError("Exact name context has conflicting human selections")
        chosen, reason = human[0], "human"
    elif policy_candidate is not None:
        chosen, reason = policy_candidate, "policy"
    elif policy.status != "excluded":
        chosen = first_counterpart(
            tuple(c for c in counterparts if c.counterpart_checked)
        )
        if chosen is not None:
            reason = "counterpart"
    if chosen is None:
        remaining = tuple(
            candidate
            for candidate in choices
            if candidate.authority == "unofficial" or policy.status != "excluded"
        )
        if remaining:
            chosen, reason = remaining[0], "choice"
    differences = tuple(
        sorted(
            {
                digest(candidate.text.encode())
                for candidate in (
                    *choices,
                    *counterparts,
                    *((policy_candidate,) if policy_candidate else ()),
                )
                if chosen is not None and candidate.text != chosen.text
            }
        )
    )
    return NameSelection(chosen, reason, differences)
