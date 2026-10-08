"""Compose authored relations with publication identity and current frozen names."""

from dataclasses import dataclass
from functools import cached_property
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build.rows import insert_exact
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import InputRecord, input_record
from sve_carddb.domains.catalog.adoption_models import Batch, ReviewContext, SourceRef
from sve_carddb.domains.digital.links.evidence import (
    Evidence,
    batch_refs,
    configured_refs,
    inventory,
)
from sve_carddb.domains.digital.links.loader import (
    Snapshot,
    decision_id,
    link_id,
    load_links,
)
from sve_carddb.domains.digital.links.models import Record, Shard, SveName
from sve_carddb.domains.translations.digital import import_digital
from sve_carddb.domains.translations.names.sources import NameOwner, name_source
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.parse.pages.official_jp import card_url

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build import Database, Value
    from sve_carddb.core.provenance import BuildContext


@dataclass(frozen=True)
class Inputs:
    root: Path
    repository: Path
    authored_revision: str

    @cached_property
    def snapshot(self) -> Snapshot:
        """Reuse current inputs within this command."""
        return load_links(self.root)

    def load(self) -> Snapshot:
        """Reuse current inputs within this command."""
        return self.snapshot

    def configuration(self) -> dict[str, JsonValue]:
        """Declare exact authored input pins for the composing build."""
        return {
            "digital_link_authored": {
                "authored_revision": self.authored_revision,
                **self.load().pins(),
            }
        }


def review_context(sources: Sources) -> ReviewContext:
    """Read explicit frozen batches; never infer them from selected targets."""
    config = object_value(parse(sources.build.configuration.encode()))
    batches = tuple(
        Batch.model_validate_json(canonical(b))
        for b in array(config.get("digital_link_sources"))
    )
    if batches != tuple(
        sorted(set(batches), key=lambda b: canonical(b.model_dump(mode="json")))
    ):
        raise ValueError("Digital-link build batches must be sorted and unique")
    return ReviewContext(context=sources.build, source_batches=batches)


@dataclass(frozen=True)
class Result:
    record: InputRecord
    fresh: tuple[bytes, ...]
    stale: tuple[tuple[str, str], ...]

    @cached_property
    def by_owner(self) -> Mapping[tuple[str, str | None], tuple[Record, ...]]:
        """Parse current records once for all publication owners."""
        grouped: dict[tuple[str, str | None], list[Record]] = {}
        for content in self.fresh:
            record = Record.model_validate_json(content)
            subject = record.subject
            grouped.setdefault((subject.card_id, subject.face_id), []).append(record)
        return MappingProxyType(
            {key: tuple(records) for key, records in grouped.items()}
        )

    def eligible_owner(  # ruff: ignore[complex-structure,too-many-branches,too-many-locals] -- current printed state and the authored relation are independent owner proofs
        self,
        db: Database,
        sources: Sources,
        owner: NameOwner,
        *,
        name_ref: SourceRef | None = None,
    ) -> frozenset[str]:
        """Recheck this owner's own name; unknown printings never borrow current."""
        if self.record.context != sources.build:
            raise ValueError("Digital-link name proof uses another build context")
        source = name_source(db, owner)
        if source is None:
            return frozenset()
        if source.lang != "ja":
            raise ValueError("Digital-link owner requires exact Japanese name")
        revisions = {r.values["id"]: r.values for r in db.rows("face_revision")}
        if owner.kind == "face_revision":
            versions: tuple[str, ...] = (str(revisions[owner.identifier]["source_id"]),)
        else:
            versions = tuple(
                str(row.values["source_id"])
                for row in db.rows("printing_face_observation")
                if row.values["printing_id"] == owner.identifier
                and row.values["face_id"] == source.face_id
                and revisions.get(row.values["revision_id"], {}).get("name_unit_id")
                == source.unit_id
            )
        if name_ref is not None:
            if (
                name_ref.text_hash != source.source_hash
                or name_ref.parser != "translation-jp-v1"
            ):
                raise ValueError(
                    "Digital-link owner reference differs from its own name"
                )
            versions = (name_ref.source_version_id,)
        face = {"id": source.face_id, "card_id": source.card_id}
        unit = {"content_hash": source.source_hash}
        review = review_context(sources)
        evidence = Evidence(sources)
        printings = tuple(
            p
            for p in evidence.index(review).by_card.get(str(face["card_id"]), ())
            if p.region == "jp"
        )
        actual = {r.values["id"]: r.values for r in db.rows("digital_link")}
        eligible = set()
        for record in self.by_owner.get((str(face["card_id"]), str(face["id"])), ()):
            subject, value = record.subject, record.value
            if (
                value.relation != "same_card"
                or subject.face_id != face["id"]
                or subject.card_id != face["card_id"]
            ):
                continue
            if actual.get(link_id(record)) != _link_values(record):
                raise ValueError(
                    "Digital-link materialized relation differs from adoption"
                )
            if not any(
                name.name_ref.text_hash == unit["content_hash"]
                for name in value.sve_names
            ):
                continue
            if not any(
                name.lang == "ja"
                and name.phase == subject.digital_phase
                and name.name_ref.text_hash == unit["content_hash"]
                for name in value.digital_names
            ):
                continue
            proven = False
            for batch in review.source_batches:
                for printing in printings:
                    for mapping, version in (
                        (m, v) for m in printing.source_face_map for v in versions
                    ):
                        if mapping.face_id != face["id"] or (
                            owner.kind == "printing_face"
                            and printing.id != owner.identifier
                        ):
                            continue
                        ref = SourceRef(
                            batch_id=batch.batch_id,
                            source_version_id=version,
                            parser="translation-jp-v1",
                            locator=f"/faces/{mapping.source_index}/name",
                            text_hash=str(unit["content_hash"]),
                        )
                        batch_key = batch.batch_id
                        frozen = sources.batch(batch_key)
                        if (
                            ref.source_version_id not in frozen.entries
                            or frozen.descriptor(ref.source_version_id).url
                            != card_url(printing.card_no)
                        ):
                            continue
                        evidence.sve(
                            SveName(
                                printing_id=printing.id,
                                face_id=mapping.face_id,
                                name_ref=ref,
                            ),
                            record,
                            review,
                        )
                        proven = True
            if not proven:
                raise ValueError(
                    "Digital-link owner lacks frozen printing face evidence"
                )
            eligible.add(link_id(record))
        return frozenset(eligible)


def populate_links(  # ruff: ignore[complex-structure,too-many-locals] -- every relation is checked before one atomic materialization
    db: Database,
    inputs: Inputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    sources: Sources | None = None,
) -> Result:
    """Validate before materializing; the composing caller owns the transaction."""
    snapshot = inputs.load()
    config = object_value(parse(build.configuration.encode()))
    if (
        config.get("digital_link_authored")
        != inputs.configuration()["digital_link_authored"]
    ):
        raise ValueError("Build configuration does not pin digital-link authored bytes")
    current = (
        sources.stage(build)
        if sources is not None
        else Sources(stores, inputs.repository, build)
    )
    review = review_context(current)
    evidence = Evidence(current)
    registry = evidence.index(review)
    names = inventory(current, configured_refs(current))
    available = {}
    for game in sorted({r.subject.game for r in snapshot.records()}):
        available.update(
            inventory(current, batch_refs(current, review.source_batches, game))
        )
    published_cards = {r.values["id"] for r in db.rows("card")}
    published_faces = {r.values["id"]: r.values["card_id"] for r in db.rows("face")}
    fresh = []
    stale = []
    for record in snapshot.records():
        subject, value = record.subject, record.value
        if subject.card_id not in registry.cards:
            raise ValueError("Digital-link current registry card is unknown")
        if registry.cards[subject.card_id].identity_state == "retired":
            stale.append((link_id(record), "current_identity_retired"))
            continue
        if subject.card_id not in published_cards or (
            subject.face_id is not None
            and (
                subject.face_id not in registry.faces
                or registry.faces[subject.face_id].card_id != subject.card_id
                or published_faces.get(subject.face_id) != subject.card_id
            )
        ):
            stale.append((link_id(record), "outside_publication_or_face_changed"))
            continue
        recorded = evidence.validate(record, review)
        phases = {name.phase for name in value.digital_names}
        expected = {
            (phase, lang): name.text
            for (game, official, phase, lang), name in names.items()
            if game == subject.game
            and official == subject.official_id
            and phase in phases
            and name.text
        }
        known = {
            (phase, lang): name.text
            for (game, official, phase, lang), name in available.items()
            if game == subject.game
            and official == subject.official_id
            and phase in phases
            and name.text
        }
        if expected != known:
            raise ValueError("Digital-link current API name closure is incomplete")
        if recorded != expected:
            stale.append((link_id(record), "digital_names_changed_or_removed"))
            continue
        fresh.append(record)
    targets = tuple(sorted({(r.subject.game, r.subject.official_id) for r in fresh}))
    if targets:
        import_digital(
            db, current, configured_refs(current), targets, allow_subset=True
        )
    insert_raw_sources(db, (use.source for use in current.uses))
    _audit(db, snapshot, inputs, fresh)
    for record in fresh:
        insert_exact(db, "digital_link", _link_values(record), ("id",))
    uses = tuple(current.uses)
    result = input_record(build, uses)
    return Result(
        result,
        tuple(canonical(record.model_dump(mode="json")) for record in fresh),
        tuple(sorted(stale)),
    )


def import_links(
    db: Database, inputs: Inputs, *, build: BuildContext, stores: dict[str, Path]
) -> Result:
    """Own one transaction only for a standalone relation import."""
    with db.transaction():
        return populate_links(db, inputs, build=build, stores=stores)


def _audit(
    db: Database, snapshot: Snapshot, inputs: Inputs, fresh: list[Record]
) -> None:
    """Record the authored shard bytes behind each materialized relation."""
    shard_of = {}
    for path, exact, content in snapshot.shards:
        identifier = (
            "authored:digital-links:"
            + digest(canonical([inputs.authored_revision, path, digest(exact)]))[7:]
        )
        insert_exact(
            db,
            "source_record",
            {
                "id": identifier,
                "kind": "authored",
                "url": None,
                "raw_locator": None,
                "etag": None,
                "last_modified": None,
                "fetched_at": None,
                "sha256": digest(exact),
                "authored_path": "authored/" + path,
                "authored_revision": inputs.authored_revision,
                "parser_version": "digital-link-authored-v2",
            },
            ("id",),
        )
        for raw in Shard.model_validate_json(content).records:
            shard_of[canonical(raw.model_dump(mode="json"))] = identifier, path
    for record in fresh:
        source, path = shard_of[canonical(record.model_dump(mode="json"))]
        decision = decision_id(record)
        insert_exact(
            db,
            "decision",
            {
                "id": decision,
                "state": record.review_level,
                "scope": "record",
                "category": "digital_link",
                "membership_hash": None,
                "policy_id": None,
                "sample_ids": None,
                "confidence": None,
                "note": record.reason,
            },
            ("id",),
        )
        insert_exact(
            db,
            "decision_source",
            {
                "decision_id": decision,
                "source_id": source,
                "role": "digital_link_shard",
                "locator": path,
                "quote": None,
            },
            ("decision_id", "source_id", "role"),
        )


def _link_values(record: Record) -> dict[str, Value]:
    subject, value = record.subject, record.value
    digital = f"digital:{subject.game}:{subject.official_id}"
    return {
        "id": link_id(record),
        "card_id": subject.card_id,
        "face_id": subject.face_id,
        "digital_card_id": digital,
        "digital_face_id": None
        if subject.digital_phase is None
        else digital + ":" + subject.digital_phase,
        "relation": value.relation,
        "effect_similarity": value.effect_similarity,
        "decision_id": decision_id(record),
    }
