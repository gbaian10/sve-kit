"""Resolve names using the current identity and each owner's exact printed source."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.translations.current import semantic_hash
from sve_carddb.translations.current_models import (
    AssignmentRecord,
    ConceptRecord,
    TermRecord,
)
from sve_carddb.translations.models import PrintingOwner
from sve_carddb.translations.name_identity import IdentityEvidence
from sve_carddb.translations.name_sources import NameOwner, NameSource, name_source

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.translations.importer import Inputs
    from sve_carddb.translations.loader import Snapshot
    from sve_carddb.translations.sources import Sources


@dataclass(frozen=True)
class ResolvedName:
    source: NameSource
    term_id: str | None
    variant: str
    reason: Literal["selected", "missing_name_concept", "ambiguous_name_concept"]
    record_hashes: tuple[str, ...]


@dataclass(frozen=True)
class Names:
    snapshot: Snapshot
    originals: tuple[tuple[str, str], ...]

    def resolve(self, db: Database, owner: NameOwner) -> ResolvedName | None:
        """A matching text cannot borrow another card's explicit assignment."""
        source = name_source(db, owner)
        if source is None:
            return None
        records = self.snapshot.current_records()
        candidates = [
            identifier
            for identifier, text in self.originals
            if source.lang == "ja" and text == source.text
        ]
        hashes: list[str] = []
        associations = [
            r
            for r in records
            if isinstance(r, ConceptRecord)
            and r.data.subject.model_dump(mode="json")
            == {
                "card_id": source.card_id,
                "face_id": source.face_id,
                "source_lang": source.lang,
                "source_hash": source.source_hash,
            }
        ]
        if associations and associations[0].data.term_id is not None:
            candidates = [associations[0].data.term_id]
            hashes.append(semantic_hash(associations[0]))
        assignments = [
            r
            for r in records
            if isinstance(r, AssignmentRecord)
            and r.data.owner.model_dump(mode="json") == owner.payload()
            and r.data.source_hash == source.source_hash
        ]
        variant = "default"
        if assignments and assignments[0].data.concept_key is not None:
            assignment = assignments[0]
            selected = "term:" + (assignment.data.concept_key or "")
            if selected not in candidates:
                raise ValueError(
                    "Name assignment differs from exact current name concepts"
                )
            candidates = [selected]
            variant = assignment.data.variant
            hashes.append(semantic_hash(assignment))
        homonyms = sum(
            text == source.text for _, text in self.originals if source.lang == "ja"
        )
        if len(candidates) != 1 or (homonyms > 1 and variant == "default"):
            return ResolvedName(
                source,
                None,
                variant,
                "ambiguous_name_concept" if candidates else "missing_name_concept",
                tuple(hashes),
            )
        return ResolvedName(source, candidates[0], variant, "selected", tuple(hashes))


def prepare(
    snapshot: Snapshot,
    originals: dict[str, str],
    inputs: Inputs,
    sources: Sources,
    db: Database,
) -> Names:
    """Validate active associations against this build; never replay superseded bases."""
    records = snapshot.current_records()
    concepts = [r for r in records if isinstance(r, ConceptRecord)]
    if concepts:
        identity = IdentityEvidence(sources, inputs.authored_revision)
        for concept in concepts:
            lang, _, _, _ = identity.association(
                concept.data.source_ref,
                card_id=concept.data.subject.card_id,
                face_id=concept.data.subject.face_id,
            )
            if lang != concept.data.subject.source_lang:
                raise ValueError("Name concept language differs from physical source")
        sources.uses.extend(identity.uses)
    variants: dict[tuple[str, str, str], str] = {}
    for record in records:
        if isinstance(record, AssignmentRecord):
            wire = record.data.owner
            owner = (
                NameOwner("printing_face", wire.printing_id, wire.face_id)
                if isinstance(wire, PrintingOwner)
                else NameOwner("face_revision", wire.revision_id)
            )
            current = name_source(db, owner)
            # A changed source is inactive; it must not transfer to the renamed owner.
            if (
                current is not None
                and current.source_hash == record.data.source_hash
                and record.data.concept_key is not None
            ):
                variant_key = (current.lang, current.source_hash, record.data.variant)
                if (
                    variants.setdefault(variant_key, record.data.concept_key)
                    != record.data.concept_key
                ):
                    raise ValueError(
                        "Name semantic variant selects conflicting concepts"
                    )
                if current.lang != "ja" and not any(
                    r.data.subject.card_id == current.card_id
                    and r.data.subject.face_id == current.face_id
                    and r.data.subject.source_hash == current.source_hash
                    and r.data.term_id == "term:" + record.data.concept_key
                    for r in concepts
                ):
                    raise ValueError(
                        "Non-Japanese assignment requires its own current concept association"
                    )
                if (
                    current.lang == "ja"
                    and originals.get("term:" + record.data.concept_key) != current.text
                ):
                    raise ValueError(
                        "Name assignment differs from exact current name concepts"
                    )
    terms = {
        r.data.id
        for r in records
        if isinstance(r, TermRecord) and r.data.category == "card_name"
    }
    return Names(
        snapshot,
        tuple(sorted((key, value) for key, value in originals.items() if key in terms)),
    )
