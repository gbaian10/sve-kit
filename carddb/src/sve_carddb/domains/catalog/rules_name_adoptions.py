"""Explicit special construction names checked against this build's name observations."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.catalog.adoption_models import (
    AuthoredText,
    SourceRef,
    SourceText,
    TextEvidence,
)
from sve_carddb.domains.catalog.models import NameBinding
from sve_carddb.domains.catalog.records import NameRecord
from sve_carddb.domains.registry.records import FaceData, PrintingData

if TYPE_CHECKING:
    from collections.abc import Iterable

    from sve_carddb.core.regions import Region
    from sve_carddb.domains.catalog.adoption_loader import AdoptionSnapshot
    from sve_carddb.domains.catalog.adoption_models import NameSubject, ReviewContext
    from sve_carddb.domains.catalog.adoption_sources import AdoptionSources
    from sve_carddb.domains.registry.snapshot import RegistrySnapshot
    from sve_carddb.domains.text_observations.plan import TextPlan

Role = Literal["primary", "collab", "treated_as"]
_LANG = {"jp": "ja", "en": "en"}


def name_basis(pairs: Iterable[tuple[str, str]]) -> str:
    """Hash the unique ``{lang,text_hash}`` set; reprints with the same name count once."""
    unique = {(lang, text) for lang, text in pairs}
    items: list[JsonValue] = [
        {"lang": lang, "text_hash": text} for lang, text in unique
    ]
    return digest(canonical(sorted(items, key=canonical)))


@dataclass(frozen=True)
class NameAdoption:
    record_key: str
    face_id: str
    region: Region
    role: Role
    names: tuple[str, ...]
    name_basis_hash: str
    # False once the face, its card or a listed printing no longer matches the registry.
    identity_current: bool


def collab_hashes(
    snapshots: tuple[AdoptionSnapshot, ...],
) -> dict[tuple[str, str], frozenset[str]]:
    """Read declared collab name hashes without sources, for wording comparisons.

    The source closure is verified later by ``check``; a hash with no matching
    observation simply never matches a printing.
    """
    hashes: dict[tuple[str, str], set[str]] = defaultdict(set)
    for snapshot in snapshots:
        for record in snapshot.current_records():
            if (
                isinstance(record, NameRecord)
                and record.data.subject.role == "collab"
                and record.data.value is not None
            ):
                subject = record.data.subject
                hashes[subject.face_id, subject.region].update(
                    name.source_ref.text_hash
                    for name in record.data.value.names
                    if isinstance(name, SourceText)
                )
    return {key: frozenset(value) for key, value in hashes.items()}


def name_refs(record: NameRecord) -> tuple[SourceRef, ...]:
    """Every text source a construction name record cites, for the review context."""
    value = record.data.value
    refs = [e.source_ref for e in record.data.evidence if isinstance(e, TextEvidence)]
    if value is not None:
        refs.extend(n.source_ref for n in value.names if isinstance(n, SourceText))
        refs.extend(o.source_ref for o in value.observations)
    return tuple(refs)


def _sorted_unique(values: tuple[JsonValue, ...], what: str) -> None:
    keys = [canonical(value) for value in values]
    if keys != sorted(set(keys)):
        raise ValueError(f"Construction name {what} must be sorted and unique")


def _names(
    record: NameRecord, review: ReviewContext, sources: AdoptionSources
) -> tuple[str, ...]:
    subject, value = record.data.subject, record.data.value
    assert value is not None
    if subject.role == "primary":
        # Only a name no printing carries may be authored, and it needs its own evidence.
        if len(value.names) != 1 or not isinstance(value.names[0], AuthoredText):
            raise ValueError("An explicit primary name is one authored text")
        if not record.data.evidence:
            raise ValueError("An explicit primary name requires evidence")
    elif not all(isinstance(name, SourceText) for name in value.names):
        raise ValueError("Collab and treated-as names must come from sources")
    _sorted_unique(tuple(n.model_dump(mode="json") for n in value.names), "names")
    names = tuple(sources.value(name, review) for name in value.names)
    if any(name.lang != _LANG[subject.region] for name in names):
        raise ValueError("Construction name language differs from its region")
    if len({name.text for name in names}) != len(names):
        raise ValueError("Construction names repeat an exact name")
    return tuple(name.text for name in names)


def _matches(
    registry: RegistrySnapshot, printing_id: str, subject: NameSubject, card_id: str
) -> bool:
    printing = registry.records.get("printing:" + printing_id)
    data = None if printing is None else printing.data
    return (
        isinstance(data, PrintingData)
        and data.region == subject.region
        and data.card_id == card_id
        and any(m.face_id == subject.face_id for m in data.source_face_map)
    )


def check(
    record: NameRecord,
    review: ReviewContext,
    sources: AdoptionSources,
    registry: RegistrySnapshot,
) -> NameAdoption | None:
    """Verify sources and the reviewed name basis; None stops using the name."""
    subject, value = record.data.subject, record.data.value
    if value is None:
        return None
    names = _names(record, review, sources)
    _sorted_unique(
        tuple(o.model_dump(mode="json") for o in value.observations), "observations"
    )
    if value.identity_ref.face_id != subject.face_id:
        raise ValueError("Construction name identity names another face")
    card_id = value.identity_ref.card_id
    face = registry.records.get("face:" + subject.face_id)
    current = (
        face is not None
        and isinstance(face.data, FaceData)
        and face.data.card_id == card_id
    )
    observed: list[tuple[str, str]] = []
    texts: set[str] = set()
    for observation in value.observations:
        if observation.face_id != subject.face_id:
            raise ValueError("Construction name observation names another face")
        text = sources.text(observation.source_ref, review)[0]
        if text.lang != _LANG[subject.region]:
            raise ValueError("Construction name observation language mismatch")
        observed.append((text.lang, observation.source_ref.text_hash))
        texts.add(text.text)
        current = current and _matches(
            registry, observation.printing_id, subject, card_id
        )
    if name_basis(observed) != value.name_basis_hash:
        raise ValueError("Construction name basis hash differs from its observations")
    if subject.role != "primary" and not set(names) <= texts:
        raise ValueError("A collab or treated-as name is not among its observations")
    return NameAdoption(
        record_key=record.record_key,
        face_id=subject.face_id,
        region=subject.region,
        role=subject.role,
        names=names,
        name_basis_hash=value.name_basis_hash,
        identity_current=current,
    )


@dataclass(frozen=True)
class NameReport:
    applied: tuple[str, ...]
    stale: tuple[str, ...]
    unverifiable: tuple[str, ...]

    def payload(self) -> dict[str, JsonValue]:
        """Record keys only; names and card wording stay out of the build report."""
        return {
            "applied": list[JsonValue](self.applied),
            "stale": list[JsonValue](self.stale),
            "unverifiable": list[JsonValue](self.unverifiable),
        }


def evaluate(
    adoptions: tuple[NameAdoption, ...], plan: TextPlan
) -> tuple[tuple[NameBinding, ...], NameReport]:
    """Rebuild every face/region name basis and keep only unchanged adoptions.

    A changed name set or identity makes an adoption stale; a missing source makes
    it unverifiable rather than unchanged. Neither one is applied.
    """
    groups = {(group.face_id, group.region): group for group in plan.groups}
    observed: dict[tuple[str, str], list[tuple[str, str]]] = defaultdict(list)
    for item in plan.observations:
        observed[item.face_id, item.region].append(
            (_LANG[item.region], digest(item.content.name.encode()))
        )
    bindings: list[NameBinding] = []
    applied, stale, unverifiable = [], [], []
    for adoption in adoptions:
        key = adoption.face_id, adoption.region
        group = groups.get(key)
        if group is None or "missing_source" in group.reasons:
            unverifiable.append(adoption.record_key)
        elif (
            not adoption.identity_current
            or name_basis(observed[key]) != adoption.name_basis_hash
        ):
            stale.append(adoption.record_key)
        else:
            applied.append(adoption.record_key)
            bindings.extend(
                NameBinding(
                    face_id=adoption.face_id,
                    region=adoption.region,
                    official_name=name,
                    role=adoption.role,
                )
                for name in adoption.names
            )
    return tuple(bindings), NameReport(
        tuple(sorted(applied)), tuple(sorted(stale)), tuple(sorted(unverifiable))
    )
