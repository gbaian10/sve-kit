"""Compose signed relations with publication identity and one atomic provenance graph."""

import re
from dataclasses import dataclass
from functools import cached_property
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.build_db.rows import insert_exact
from sve_carddb.build_inputs import InputRecord, input_record, insert_raw_sources
from sve_carddb.catalog.adoption_models import Batch, ReviewContext, SourceRef
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.digital_links.evidence import (
    Evidence,
    batch_refs,
    configured_refs,
    inventory,
)
from sve_carddb.digital_links.loader import Snapshot, link_id, load_links
from sve_carddb.digital_links.models import Record, Shard, SveName
from sve_carddb.digital_links.models import Value as LinkValue
from sve_carddb.maintainers import is_maintainer
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.sources.official_jp import card_url
from sve_carddb.translations.digital import import_digital
from sve_carddb.translations.name_sources import NameOwner, name_source
from sve_carddb.translations.sources import Sources

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build_db import Database, Value
    from sve_carddb.build_inputs import BuildContext


@dataclass(frozen=True)
class Inputs:
    root: Path
    repository: Path
    authored_revision: str

    def load(self) -> Snapshot:
        """Require a clean immutable authored entry, not merely matching canonical data."""
        if re.fullmatch(r"[0-9a-f]{40}", self.authored_revision) is None:
            raise ValueError("Digital-link authored revision must be full Git SHA")
        snapshot = load_links(self.root)
        repository = PinnedRepository(self.repository)
        for name, exact in [
            ("digital-links/index.yaml", snapshot.index),
            *((p, e) for p, e, _ in snapshot.shards),
        ]:
            if repository.read(self.authored_revision, "authored/" + name) != exact:
                raise ValueError(
                    "Digital-link bytes differ from immutable authored revision"
                )
        return snapshot

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
    stale: tuple[str, ...]
    withdrawn: tuple[str, ...]
    decisions: tuple[tuple[str, str], ...]
    stale_reasons: tuple[tuple[str, str], ...]
    checked_members: frozenset[str] = frozenset()

    @cached_property
    def by_owner(self) -> Mapping[tuple[str, str | None], tuple[Record, ...]]:
        """Parse immutable terminal records once for all publication owners."""
        grouped: dict[tuple[str, str | None], list[Record]] = {}
        for content in self.fresh:
            record = Record.model_validate_json(content)
            subject = record.data.subject
            grouped.setdefault((subject.card_id, subject.face_id), []).append(record)
        return MappingProxyType(
            {key: tuple(records) for key, records in grouped.items()}
        )

    def eligible(
        self, db: Database, sources: Sources, revision_id: str
    ) -> frozenset[str]:
        """Keep the existing exact-revision API for legacy name materialization."""
        return self.eligible_owner(db, sources, NameOwner("face_revision", revision_id))

    def eligible_owner(  # ruff: ignore[complex-structure,too-many-branches,too-many-locals] -- current printed state and frozen adoption are independent owner proofs
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
        decisions = dict(self.decisions)
        eligible = set()
        for record in self.by_owner.get((str(face["card_id"]), str(face["id"])), ()):
            subject, value = record.data.subject, record.data.value
            if (
                value is None
                or value.relation != "same_card"
                or subject.face_id != face["id"]
                or subject.card_id != face["card_id"]
            ):
                continue
            if actual.get(link_id(record)) != _link_values(
                record, decisions[record.record_key], value
            ):
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


def populate_links(  # ruff: ignore[complex-structure,too-many-branches,too-many-statements,too-many-locals] -- history and current closure are checked before one atomic materialization
    db: Database, inputs: Inputs, *, build: BuildContext, stores: dict[str, Path]
) -> Result:
    """Validate before materializing; the composing caller owns the transaction."""
    snapshot = inputs.load()
    config = object_value(parse(build.configuration.encode()))
    if (
        config.get("digital_link_authored")
        != inputs.configuration()["digital_link_authored"]
    ):
        raise ValueError("Build configuration does not pin digital-link authored bytes")
    current = Sources(stores, inputs.repository, build)
    review = review_context(current)
    current_registry = Evidence(current).index(review)
    names = inventory(current, configured_refs(current))
    available = {}
    for game in sorted({r.data.subject.game for r, _ in snapshot.records()}):
        available.update(
            inventory(current, batch_refs(current, review.source_batches, game))
        )
    history: dict[str, dict[tuple[str, str], str]] = {}
    resolved = []
    for shard in snapshot.envelopes():
        # Historical pins prove the review; current code revalidates its frozen evidence.
        sources = Sources(
            stores, inputs.repository, shard.review_context.context, historical=True
        )
        evidence = Evidence(sources)
        for record in shard.records:
            history[record.record_key] = evidence.validate(record, shard.review_context)
        resolved.append((shard, sources))
    cards, faces = current_registry.cards, current_registry.faces
    published_cards = {r.values["id"] for r in db.rows("card")}
    published_faces = {r.values["id"]: r.values["card_id"] for r in db.rows("face")}
    fresh = []
    stale = []
    stale_reasons = []
    withdrawn = []
    for record, decision in snapshot.effective():
        value = record.data.value
        if value is None:
            withdrawn.append(record.record_key)
            continue
        subject = record.data.subject
        if subject.card_id not in cards:
            raise ValueError("Digital-link current registry card is unknown")
        if cards[subject.card_id].identity_state == "retired":
            stale.append(record.record_key)
            stale_reasons.append((record.record_key, "current_identity_retired"))
            continue
        if subject.card_id not in published_cards or (
            subject.face_id is not None
            and (
                subject.face_id not in faces
                or faces[subject.face_id].card_id != subject.card_id
                or published_faces.get(subject.face_id) != subject.card_id
            )
        ):
            stale.append(record.record_key)
            stale_reasons.append(
                (record.record_key, "outside_publication_or_face_changed")
            )
            continue
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
        if history[record.record_key] != expected:
            stale.append(record.record_key)
            stale_reasons.append(
                (record.record_key, "digital_names_changed_or_removed")
            )
            continue
        fresh.append((record, decision, value))
    targets = tuple(
        sorted({(r.data.subject.game, r.data.subject.official_id) for r, _, _ in fresh})
    )
    if targets:
        import_digital(
            db, current, configured_refs(current), targets, allow_subset=True
        )
    for _, sources in resolved:
        insert_raw_sources(db, (use.source for use in sources.uses))
    insert_raw_sources(db, (use.source for use in current.uses))
    _audit(db, snapshot, inputs, resolved)
    for record, decision, value in fresh:
        insert_exact(db, "digital_link", _link_values(record, decision, value), ("id",))
    uses = tuple(use for _, sources in resolved for use in sources.uses) + tuple(
        current.uses
    )
    result = input_record(build, uses)
    result.verify(db, build, uses, complete=False)
    return Result(
        result,
        tuple(canonical(record.model_dump(mode="json")) for record, _, _ in fresh),
        tuple(sorted(stale)),
        tuple(sorted(withdrawn)),
        tuple((record.record_key, decision) for record, decision, _ in fresh),
        tuple(sorted(stale_reasons)),
        frozenset(
            key
            for shard, _ in resolved
            for decision in shard.decisions
            for key in decision.sample_ids
            if is_maintainer(decision.reviewed_by)
            and decision.state in {"sampled", "confirmed"}
        ),
    )


def import_links(
    db: Database, inputs: Inputs, *, build: BuildContext, stores: dict[str, Path]
) -> Result:
    """Own one transaction only for a standalone relation import."""
    with db.transaction():
        return populate_links(db, inputs, build=build, stores=stores)


def _audit(
    db: Database,
    snapshot: Snapshot,
    inputs: Inputs,
    resolved: list[tuple[Shard, Sources]],
) -> None:
    index_id = (
        "authored:digital-links:"
        + digest(
            canonical(
                [
                    inputs.authored_revision,
                    "digital-links/index.yaml",
                    digest(snapshot.index),
                ]
            )
        )[7:]
    )
    insert_exact(
        db,
        "source_record",
        {
            "id": index_id,
            "kind": "authored",
            "url": None,
            "raw_locator": None,
            "etag": None,
            "last_modified": None,
            "fetched_at": None,
            "sha256": digest(snapshot.index),
            "authored_path": "authored/digital-links/index.yaml",
            "authored_revision": inputs.authored_revision,
            "parser_version": "digital-link-authored-v1",
        },
        ("id",),
    )
    for (path, exact, _), (shard, sources) in zip(
        snapshot.shards, resolved, strict=True
    ):
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
                "parser_version": "digital-link-authored-v1",
            },
            ("id",),
        )
        decision = shard.decisions[0]
        values: dict[str, Value] = {
            k: v
            for k, v in decision.model_dump(
                mode="json", exclude={"members", "sample_ids", "reviewed_precision"}
            ).items()
            if isinstance(v, str) or v is None
        }
        values["sample_ids"] = Json(list[JsonValue](decision.sample_ids))
        values["confidence"] = None
        insert_exact(db, "decision", values, ("id",))
        registry_sources = _registry_audit(db, sources)
        for source_id, locator, role in [
            *registry_sources,
            (identifier, path, "digital_link_envelope"),
            (index_id, "digital-links/index.yaml", "digital_link_index"),
            *(
                (
                    use.source.id,
                    use.locator,
                    "digital_link_evidence:"
                    + digest(canonical([use.source.id, use.locator]))[7:],
                )
                for use in sources.uses
            ),
        ]:
            insert_exact(
                db,
                "decision_source",
                {
                    "decision_id": decision.id,
                    "source_id": source_id,
                    "role": role,
                    "locator": locator,
                    "quote": None,
                },
                ("decision_id", "source_id", "role"),
            )


def _registry_audit(db: Database, sources: Sources) -> list[tuple[str, str, str]]:
    pin = object_value(
        object_value(parse(sources.build.configuration.encode()))["catalog_registry"]
    )
    revision = str(pin["authored_revision"])
    registry = Evidence(sources).registry(review_context(sources))
    names = ["ids/index.yaml", *(shard.path for shard in registry.files.shards)]
    result = []
    for path in names:
        raw = sources.repository.read(revision, "authored/" + path)
        identifier = (
            "authored:digital-registry:"
            + digest(canonical([revision, path, digest(raw)]))[7:]
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
                "sha256": digest(raw),
                "authored_path": "authored/" + path,
                "authored_revision": revision,
                "parser_version": "digital-link-registry-v1",
            },
            ("id",),
        )
        result.append(
            (identifier, path, "digital_link_registry:" + digest(path.encode())[7:])
        )
    return result


def _link_values(record: Record, decision: str, value: LinkValue) -> dict[str, Value]:
    subject = record.data.subject
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
        "decision_id": decision,
    }
