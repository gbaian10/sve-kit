"""Frozen name eligibility and card-level browsing plans; no database population."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, model_validator

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.provenance import SourceUse, uses_sorted
from sve_carddb.domains.catalog.adoption_models import ReviewContext, SourceRef
from sve_carddb.domains.digital.links.evidence import Evidence
from sve_carddb.domains.registry.projection import jp_card
from sve_carddb.domains.registry.records import CardId, FaceId, PrintingId
from sve_carddb.domains.registry.review import observation
from sve_carddb.parse.pages.extract_jp import extract_card
from sve_carddb.parse.pages.official_jp import card_url

if TYPE_CHECKING:
    from sve_carddb.domains.digital.links.evidence import Name
    from sve_carddb.domains.digital.name_policies.catalogue import Catalogue
    from sve_carddb.domains.translations.sources import Sources

# The same-name rule lives in code; authored data only lists source batches and
# exclusions, so changing the rule itself is a reviewed program change.
LINK_RELATION = "same_name"
LINK_GAMES = ("sv1", "svwb")


class NameOwner(RecordData):
    kind: Literal["face_revision", "printing_face"]
    owner_id: Text
    card_id: CardId
    face_id: FaceId
    printing_id: PrintingId
    state: Literal["known", "unknown"]
    name_ref: SourceRef | None

    @model_validator(mode="after")
    def known_source(self) -> NameOwner:
        """Unknown owners cannot borrow a current name."""
        if (self.state == "known") != (self.name_ref is not None):
            raise ValueError(
                "Policy name owner must have its own known source or be unknown"
            )
        return self


@dataclass(frozen=True)
class OwnerEvidence:
    owner: NameOwner
    text: str | None
    uses: tuple[SourceUse, ...]
    context_hash: str


@dataclass(frozen=True)
class FrozenName:
    game: str
    official_id: str
    phase: str
    lang: str
    text: str
    ref: SourceRef


@dataclass(frozen=True)
class NamePolicyResult:
    owner: NameOwner
    context_hash: str
    policy_hash: str
    source_name_hash: str | None
    status: Literal["eligible", "untranslated", "excluded", "unknown"]
    condition: str
    game: str | None
    text: str | None
    refs: tuple[SourceRef, ...]
    uses: tuple[SourceUse, ...]


@dataclass(frozen=True)
class RuleLinkPlan:
    card_id: str
    game: str
    official_id: str
    policy_hash: str
    context_hash: str
    owners: tuple[NameOwner, ...]
    refs: tuple[SourceRef, ...]
    uses: tuple[SourceUse, ...]

    def subject(self) -> dict[str, JsonValue]:
        """Keep plans at card level, independent of phase evidence."""
        return {
            "card_id": self.card_id,
            "face_id": None,
            "game": self.game,
            "official_id": self.official_id,
            "digital_phase": None,
        }


def _parents(names: dict[tuple[str, str, str, str], Name], game: str) -> None:
    available = {
        identifier for (g, identifier, _, lang) in names if g == game and lang == "ja"
    }
    for name in names.values():
        for field in ("base_card_id", "original_card_id"):
            parent = name.common.get(field)
            if parent is not None and (
                type(parent) is not int or (parent and str(parent) not in available)
            ):
                raise ValueError("Digital-name catalogue parent closure mismatch")


def owner_text(
    owner: NameOwner, sources: Sources, review: ReviewContext
) -> OwnerEvidence:
    """Validate the owner's own registry, whole identity observation, face and exact field."""
    if sources.build != review.context:
        raise ValueError("Digital-name owner resolver differs from build context")
    evidence = Evidence(sources)
    index = evidence.index(review)
    printing = index.printings.get(owner.printing_id)
    face = index.faces.get(owner.face_id)
    if (
        printing is None
        or face is None
        or owner.card_id not in index.cards
        or printing.card_id != owner.card_id
        or face.card_id != owner.card_id
    ):
        raise ValueError("Digital-name owner identity or parent card mismatch")
    if owner.state == "unknown":
        return OwnerEvidence(
            owner, None, (), digest(sources.context_key(review.context))
        )
    ref = owner.name_ref
    assert ref is not None
    if ref.batch_id not in {b.batch_id for b in review.source_batches}:
        raise ValueError("Digital-name owner source is outside build closure")
    lang, text, source = sources.text(ref)
    if (
        printing.region != "jp"
        or ref.parser != "translation-jp-v1"
        or lang != "ja"
        or source.url != card_url(printing.card_no)
        or ref.locator
        not in {
            f"/faces/{m.source_index}/name"
            for m in printing.source_face_map
            if m.face_id == owner.face_id
        }
    ):
        raise ValueError("Digital-name owner printing face source mismatch")
    _, raw, _ = sources.batch(ref.batch_id).read(
        ref.source_version_id, parser_version=ref.parser
    )
    actual = observation(jp_card(extract_card(raw, number=printing.card_no)), "jp")
    if actual != printing.observation.model_dump(mode="json"):
        raise ValueError("Digital-name owner registry observation mismatch")
    use = SourceUse(
        source=source,
        usage="digital_policy_owner_name",
        locator=canonical(owner.model_dump(mode="json")).decode(),
    )
    sources.uses.append(use)
    return OwnerEvidence(
        owner, text, (use,), digest(sources.context_key(review.context))
    )


def name_result(evidence: OwnerEvidence, frozen: Catalogue) -> NamePolicyResult:
    """Pure eligibility after owner evidence validation; this is never human review."""
    owner, text, owner_uses = evidence.owner, evidence.text, evidence.uses
    if frozen.purpose != "names":
        raise ValueError("Browsing policy cannot supply an official name")
    status: Literal["eligible", "untranslated", "excluded", "unknown"] = "untranslated"
    condition, game, translated = "no_exact_japanese_name", None, None
    refs: tuple[SourceRef, ...] = ()
    checksum = None if text is None else digest(text.encode())
    if text is None:
        status, condition = "unknown", "unknown_owner_name"
    elif checksum in frozen.excluded_names:
        status, condition = "excluded", "name_exclusion"
    else:
        for provider in ("sv1", "svwb"):
            matches = frozen.matching(text, provider)
            if not matches:
                continue
            condition = "nonunique_or_missing_translation"
            targets = {(m.official_id, m.phase) for m in matches}
            target_names = tuple(
                frozen.translated[provider, identifier, phase]
                for identifier, phase in sorted(targets)
                if (provider, identifier, phase) in frozen.translated
            )
            good = len(target_names) == len(targets) and all(
                n.text
                and not all(ord(c) in frozen.whitespace for c in n.text)
                and not any(
                    start <= ord(c) <= end for c in n.text for start, end in frozen.kana
                )
                for n in target_names
            )
            unique = {n.text for n in target_names}
            if good and len(unique) == 1:
                status, condition, game, translated = (
                    "eligible",
                    "unique_frozen_official_name",
                    provider,
                    next(iter(unique)),
                )
                refs = tuple(
                    sorted(
                        {n.ref for n in (*matches, *target_names)},
                        key=lambda r: canonical(r.model_dump(mode="json")),
                    )
                )
            # A present but ineligible first-generation name cannot borrow svwb.
            break
    return NamePolicyResult(
        owner,
        evidence.context_hash,
        frozen.policy_hash,
        checksum,
        status,
        condition,
        game,
        translated,
        refs,
        (*frozen.uses, *owner_uses),
    )


def rule_links(
    owners: tuple[OwnerEvidence, ...], frozen: Catalogue
) -> tuple[RuleLinkPlan, ...]:
    """Propose deduplicated card/game/ID plans, never same_card human records."""
    if frozen.purpose != "links":
        raise ValueError("Name policy cannot authorize browsing links")
    groups: dict[
        tuple[str, str, str], list[tuple[NameOwner, tuple[SourceUse, ...], FrozenName]]
    ] = defaultdict(list)
    contexts = {e.context_hash for e in owners}
    if len(contexts) > 1:
        raise ValueError("Digital-name link owners belong to different build contexts")
    for evidence in owners:
        owner, text, uses = evidence.owner, evidence.text, evidence.uses
        if text is None or not text or digest(text.encode()) in frozen.excluded_names:
            continue
        for game in LINK_GAMES:
            for match in frozen.matching(text, game):
                key = owner.card_id, game, match.official_id
                if key not in frozen.excluded_targets:
                    groups[key].append((owner, uses, match))
    return tuple(
        RuleLinkPlan(
            card,
            game,
            official,
            frozen.policy_hash,
            next(iter(contexts)),
            tuple(
                sorted(
                    {o for o, _, _ in members},
                    key=lambda o: canonical(o.model_dump(mode="json")),
                )
            ),
            tuple(
                sorted(
                    {
                        ref
                        for o, _, match in members
                        for ref in (o.name_ref, match.ref)
                        if ref is not None
                    },
                    key=lambda r: canonical(r.model_dump(mode="json")),
                )
            ),
            uses_sorted(
                (*frozen.uses, *(use for _, uses, _ in members for use in uses))
            ),
        )
        for (card, game, official), members in sorted(groups.items())
    )
