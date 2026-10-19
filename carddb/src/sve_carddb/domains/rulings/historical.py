"""Validate historical data at admission; rebuild from the reviewed use list."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.contracts.rulings import LegacyReference
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.rulings.mapping import (
    Definition,
    FrameUse,
    LegacyInputs,
    LegacyUse,
    Namespace,
    Rebuild,
    key,
    source_scope,
)
from sve_carddb.domains.translations.four_layer_sources import card_field

if TYPE_CHECKING:
    from collections.abc import Callable

    from sve_carddb.build import Database
    from sve_carddb.contracts.rulings import HistoricalUse
    from sve_carddb.domains.effects.reader import Catalog
    from sve_carddb.domains.rulings.reader import Document
    from sve_carddb.domains.translations.four_layer_pipeline import CompiledField

V0 = Namespace(
    code="classification-v0",
    id_recipe="v0-normalized-sha256-prefix10",
    normalizer_version="v0-index",
)


@dataclass(frozen=True)
class Observation:
    text: str
    face: int
    cards: frozenset[str]


def references(documents: tuple[Document, ...]) -> tuple[LegacyReference, ...]:
    """The same old definition cannot acquire different domains in different rulings."""
    result: dict[str, LegacyReference] = {}
    bare: set[str] = set()
    for document in documents:
        for reference in document.ruling.applies_to:
            if isinstance(reference, LegacyReference):
                previous = result.setdefault(reference.legacy, reference)
                if previous != reference:
                    raise ValueError("Conflicting authored historical domains")
            else:
                bare.add(reference)
    if bare & result.keys():
        raise ValueError("Mixed bare and explicit historical references")
    return tuple(result[k] for k in sorted(result))


def verify_use(use: HistoricalUse, observed: Observation) -> str:
    """A missing binding is pending; false source facts cannot be downgraded to pending."""
    occurrence = use.occurrence
    raw = observed.text
    if occurrence.source_hash != digest(raw.encode())[7:]:
        raise ValueError("Historical source field hash differs")
    if not set(use.cards) <= observed.cards:
        raise ValueError("Historical printing does not belong to the source face")
    lines = raw.split("\n")
    if occurrence.line_ordinal >= len(lines):
        raise ValueError("Historical source line is absent")
    start = sum(map(len, lines[: occurrence.line_ordinal])) + occurrence.line_ordinal
    end = start + len(lines[occurrence.line_ordinal])
    if any(s.start < start or s.end > end for s in occurrence.segments):
        raise ValueError("Historical source segments cross their exact line")
    return "".join(raw[s.start : s.end] for s in occurrence.segments)


def inputs(
    documents: tuple[Document, ...],
) -> LegacyInputs:
    """Read admitted definitions without replaying source or catalog validation."""
    definitions = []
    for reference in references(documents):
        uses = tuple(
            LegacyUse(
                occurrence=use.occurrence,
                scope=source_scope(use.occurrence),
                parameters={},
            )
            for use in reference.uses
        )
        definitions.append(
            Definition(
                template_id=reference.legacy,
                uses=uses,
                semantic_variant="classification_v0." + reference.legacy.lower(),
            )
        )
    verified = {d.template_id: d for d in definitions}

    def verify(definition: Definition) -> None:
        if verified.get(definition.template_id) != definition:
            raise ValueError("Historical definition differs from verified inputs")

    return LegacyInputs(V0, tuple(definitions), verify)


def validate(
    documents: tuple[Document, ...],
    catalog: Catalog,
    observe: Callable[[HistoricalUse], Observation],
) -> None:
    """At data admission every v0 member must occur exactly once with its old body hash."""
    if catalog.namespace != V0.code:
        raise ValueError("Historical catalog namespace differs")
    members = {m.id: m for m in catalog.members}
    classifications = {c.id: c for c in catalog.classifications}
    for reference in references(documents):
        classification = classifications.get(reference.legacy)
        if classification is None:
            raise ValueError("Historical definition is absent from the catalog")
        expected = {
            canonical(members[i].position.model_dump(mode="json")): members[i]
            for i in classification.members
        }
        seen: set[bytes] = set()
        for use in reference.uses:
            observed = observe(use)
            body = verify_use(use, observed)
            occurrence = use.occurrence
            for card in use.cards:
                position = canonical(
                    {
                        "region": "jp",
                        "card_no": card,
                        "face": observed.face,
                        "field": occurrence.field,
                        "ordinal": occurrence.ordinal,
                        "line": occurrence.line_ordinal,
                    }
                )
                member = expected.get(position)
                if member is None or position in seen:
                    raise ValueError(
                        "Historical domain adds or repeats a catalog member"
                    )
                if member.body_hash != digest(
                    body.encode()
                ) or member.body_length != len(body):
                    raise ValueError(
                        "Historical source span differs from the catalog body"
                    )
                seen.add(position)
        if seen != expected.keys():
            raise ValueError("Historical domain omits catalog members")


class Observations:
    def __init__(self, db: Database) -> None:
        self.db = db
        self.cache: dict[bytes, Observation] = {}

    def __call__(self, use: HistoricalUse) -> Observation:
        """Read exact owner fields from the selected, previously validated offline build."""
        occurrence = use.occurrence
        if not isinstance(occurrence.owner, FaceRevisionOwner):
            raise TypeError("Classification-v0 requires a revision owner")
        field = OwnerField(
            owner=occurrence.owner, field=occurrence.field, ordinal=occurrence.ordinal
        )
        encoded = key(field)
        if encoded not in self.cache:
            source = card_field(self.db, field)
            face = self.db.select("face", ("ordinal",), where={"id": source.face_id})[
                0
            ].values["ordinal"]
            if type(face) is not int:
                raise TypeError("Historical face ordinal must be an integer")
            printings = {
                row.values["id"]: row.values["card_no"]
                for row in self.db.select(
                    "printing",
                    ("id", "card_no"),
                    where={"card_id": source.card_id, "region": "jp"},
                )
            }
            cards = frozenset(
                str(printings[row.values["printing_id"]])
                for row in self.db.select(
                    "printing_face", ("printing_id",), where={"face_id": source.face_id}
                )
                if row.values["printing_id"] in printings
            )
            self.cache[encoded] = Observation(source.text, face, cards)
        return self.cache[encoded]


def rebuild(
    documents: tuple[Document, ...],
    compiled: tuple[CompiledField, ...],
) -> Rebuild:
    """Only the translation build's already verified and persisted bindings are candidates."""
    old = inputs(documents)
    current = {
        match.binding.id: FrameUse(
            match.frame,
            match.binding,
            source_scope(match.binding.occurrence_key()),
        )
        for field in compiled
        for match in field.matches
        if match is not None
    }

    def verify(use: FrameUse) -> None:
        if current.get(use.binding.id) != use:
            raise ValueError("Ruling candidate is outside the verified build")

    return Rebuild((old,), tuple(current.values()), verify)
