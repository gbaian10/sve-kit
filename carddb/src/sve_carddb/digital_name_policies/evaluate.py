"""Frozen name eligibility and card-level browsing plans; no database population."""

from collections import defaultdict
from dataclasses import dataclass
from functools import cached_property
from types import MappingProxyType
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, model_validator

from sve_carddb.build_inputs import BuildContext, SourceUse, uses_sorted
from sve_carddb.catalog.adoption_models import ReviewContext, SourceRef
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.digital_links.catalogue import complete_inventory
from sve_carddb.digital_links.evidence import Evidence
from sve_carddb.digital_name_policies.models import CardTargetExclusion, NameExclusion
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.registry.records import CardId, FaceId, PrintingId, RecordData, Text
from sve_carddb.registry.review import observation
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.sources.official_jp import card_url
from sve_carddb.translations.sources import Sources

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.digital_links.evidence import Name
    from sve_carddb.digital_name_policies.current_evaluate import (
        Catalogue as CurrentCatalogue,
    )
    from sve_carddb.digital_name_policies.loader import LoadedPolicy


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
class Catalogue:
    policy_hash: str
    purpose: str
    names: tuple[FrozenName, ...]
    uses: tuple[SourceUse, ...]
    whitespace: frozenset[int]
    kana: tuple[tuple[int, int], ...]
    excluded_names: frozenset[str]
    excluded_targets: frozenset[tuple[str, str, str]]

    @cached_property
    def japanese(self) -> Mapping[tuple[str, str], tuple[FrozenName, ...]]:
        """Index hashes once while retaining complete strings for collision checks."""
        groups: dict[tuple[str, str], list[FrozenName]] = defaultdict(list)
        for name in self.names:
            if name.lang == "ja" and name.text:
                groups[name.game, digest(name.text.encode())].append(name)
        return MappingProxyType({key: tuple(values) for key, values in groups.items()})

    @cached_property
    def translated(self) -> Mapping[tuple[str, str, str], FrozenName]:
        """Retain every target and phase instead of a selected-target subset."""
        return MappingProxyType(
            {
                (n.game, n.official_id, n.phase): n
                for n in self.names
                if n.lang == "zh-Hant"
            }
        )

    def matching(self, text: str, game: str) -> tuple[FrozenName, ...]:
        """Hash hits still require exact complete strings."""
        return tuple(
            n
            for n in self.japanese.get((game, digest(text.encode())), ())
            if n.text == text and text
        )


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


def historical_sources(
    loaded: LoadedPolicy, stores: Mapping[str, Path], repository: Path
) -> Sources:
    """Historical provenance uses immutable Git dependencies, never current runtime bytes."""
    pins = loaded.catalogue()
    pinned = PinnedRepository(repository)
    names: list[str] = []
    # The historical parser's existing dependency list is itself an immutable Git blob.
    # Read the complete historical Python package, avoiding dependency growth on upgrade.
    import subprocess  # ruff: ignore[import-outside-top-level,suspicious-subprocess-import] -- immutable dependency enumeration belongs to this replay boundary

    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- validated full SHA and fixed paths, no shell
        [
            pinned.executable,
            "-C",
            str(repository),
            "ls-tree",
            "-r",
            "--name-only",
            pins.count_replay_main_revision,
            "--",
            "carddb/src/sve_carddb",
            "carddb/uv.lock",
            "carddb/pyproject.toml",
        ],
        check=False,
        capture_output=True,
    )
    if result.returncode:
        raise ValueError("Digital-name historical program tree is unavailable")
    names.extend(
        n
        for n in result.stdout.decode().splitlines()
        if n.endswith((".py", "uv.lock", "pyproject.toml"))
    )
    dependencies = pinned.read_many(pins.count_replay_main_revision, tuple(names))
    context = BuildContext.from_inputs(
        pins.count_replay_main_revision,
        dependencies,
        pins.parser_and_registry_configuration.model_dump(mode="json"),
    )
    return Sources(dict(stores), repository, context, historical=True)


def catalogue(loaded: LoadedPolicy, sources: Sources) -> Catalogue:
    """Replay both entire pinned games; missing translations remain explicit members."""
    document = loaded.document()
    pins = loaded.catalogue()
    if (
        object_value(parse(sources.build.configuration.encode()))
        != pins.parser_and_registry_configuration.model_dump(mode="json")
        or sources.build.program_revision != pins.count_replay_main_revision
    ):
        raise ValueError("Digital-name catalogue differs from approved frozen pins")
    review = ReviewContext(context=sources.build, source_batches=pins.source_batches)
    registry = Evidence(sources).index(review)
    names: list[FrozenName] = []
    for game in ("sv1", "svwb"):
        full = complete_inventory(sources, review, game)
        _parents(full, game)
        names.extend(
            FrozenName(n.game, n.official_id, n.phase, n.lang, n.text, n.ref)
            for _, n in sorted(full.items())
        )
    excluded = loaded.excluded()
    hashes = {digest(n.text.encode()) for n in names if n.lang == "ja" and n.text}
    name_exclusions = frozenset(
        item.source_name_hash
        for item in excluded.entries
        if isinstance(item, NameExclusion)
    )
    if not name_exclusions <= hashes:
        raise ValueError(
            "Digital-name exclusion cannot locate its frozen Japanese name"
        )
    targets = frozenset(
        (item.card_id, item.game, item.official_id)
        for item in excluded.entries
        if isinstance(item, CardTargetExclusion)
    )
    if any(
        card not in registry.cards
        or not any(
            n.game == game and n.official_id == official and n.lang == "ja"
            for n in names
        )
        for card, game, official in targets
    ):
        raise ValueError("Digital-name exclusion cannot locate its card target")
    return Catalogue(
        digest(loaded.policy),
        document.purpose,
        tuple(names),
        uses_sorted(sources.uses),
        frozenset(),
        (),
        name_exclusions,
        targets,
    )


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
    actual = observation(
        legacy_projection(extract_card(raw, number=printing.card_no)), "jp"
    )
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


def name_result(
    evidence: OwnerEvidence, frozen: Catalogue | CurrentCatalogue
) -> NamePolicyResult:
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
    owners: tuple[OwnerEvidence, ...], frozen: Catalogue | CurrentCatalogue
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
        for game in ("sv1", "svwb"):
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
