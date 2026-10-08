"""Atomic identity staging into the typed build database boundary."""

import re
from typing import TYPE_CHECKING

from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import (
    BuildContext,
    InputRecord,
    SourceUse,
    input_record,
)
from sve_carddb.registry.inputs import digest
from sve_carddb.registry.records import (
    AllocationData,
    ArtData,
    CardData,
    FaceData,
    MappingReviewData,
    PrintingData,
    RelatedData,
)
from sve_carddb.routes import populate_routes

if TYPE_CHECKING:
    from sve_carddb.build import Value
    from sve_carddb.build.database import Database
    from sve_carddb.registry.preview.plan import PreviewPlan
    from sve_carddb.registry.snapshot import RegistryRecord


def import_preview(
    db: Database, plan: PreviewPlan, *, authored_revision: str, build: BuildContext
) -> InputRecord:
    """Own one transaction; missing supplied product parents fail without partial import."""
    with db.transaction():
        return populate_preview(
            db, plan, authored_revision=authored_revision, build=build
        )


def populate_preview(
    db: Database, plan: PreviewPlan, *, authored_revision: str, build: BuildContext
) -> InputRecord:
    """Populate inside a caller-owned transaction, including rebuild_database callbacks."""
    inputs = populate_identity_rows(
        db, plan, authored_revision=authored_revision, build=build
    )
    populate_routes(db)
    return inputs


def populate_identity_rows(
    db: Database, plan: PreviewPlan, *, authored_revision: str, build: BuildContext
) -> InputRecord:
    """Stage identities before checked same-number overrides in one owned transaction."""
    if not re.fullmatch(r"[0-9a-f]{40}", authored_revision):
        raise ValueError("Authored revision must be a full Git commit SHA")
    parents = {row.values["id"] for row in db.rows("product_family")}
    required = {
        record.data.home_set_id
        for kind in ("card", "printing")
        for record in plan.included(kind)
        if isinstance(record.data, (CardData, PrintingData))
    }
    if required - parents:
        raise ValueError(
            "Missing product_family parents; supply verified product data first"
        )
    authored = _authored(db, plan, authored_revision)
    uses = _evidence(db, plan)
    for kind in (
        "card",
        "face",
        "card_int_id",
        "art",
        "region_mapping_review",
        "card_related",
    ):
        for record in plan.included(kind):
            db.insert(kind, _identity(record, authored[record.shard_path]))
    art_uses: dict[tuple[str, str], str] = {}
    for record in plan.included("art"):
        data = record.data
        if isinstance(data, ArtData):
            for use in data.uses:
                key = use.printing_id, use.face_id
                if key in art_uses:
                    raise ValueError(
                        "Multiple adopted art groups for one printing face"
                    )
                art_uses[key] = data.id
    for record in plan.included("printing"):
        _printing(db, plan, record, art_uses)
    return input_record(build, uses)


def _authored(db: Database, plan: PreviewPlan, revision: str) -> dict[str, str]:
    sources: dict[str, str] = {}
    for shard in plan.snapshot.files.shards:
        source_id = "authored:v1:" + digest(
            {
                "path": shard.path,
                "hash": shard.content_hash,
                "revision": revision,
            }
        ).removeprefix("sha256:")
        sources[shard.path] = source_id
        db.insert(
            "source_record",
            {
                "id": source_id,
                "kind": "authored",
                "sha256": shard.content_hash,
                "authored_path": "authored/" + shard.path,
                "authored_revision": revision,
                "parser_version": "registry-envelope-v1",
            },
        )
    return sources


def _evidence(db: Database, plan: PreviewPlan) -> tuple[SourceUse, ...]:
    uses = tuple(
        SourceUse(
            source=item.source,
            usage="registry_observation",
            locator=canonical({"region": region, "card_no": number}).decode(),
        )
        for (region, number), item in sorted(plan.evidence.items())
    )
    insert_raw_sources(db, (use.source for use in uses))
    return uses


def _identity(record: RegistryRecord, source_id: str) -> dict[str, Value]:
    data = record.data
    if isinstance(data, CardData):
        return {
            "id": data.id,
            "layout": data.layout,
            "identity_state": data.identity_state,
            "home_set_id": data.home_set_id,
        }
    if isinstance(data, FaceData):
        return {
            "id": data.id,
            "card_id": data.card_id,
            "ordinal": data.ordinal,
            "side": data.side,
        }
    if isinstance(data, AllocationData):
        return {
            "int_id": data.int_id,
            "printing_id": data.printing_id,
        }
    if isinstance(data, ArtData):
        return {
            "id": data.id,
            "card_id": data.card_id,
            "face_id": data.face_id,
            "classification": data.classification,
        }
    if isinstance(data, MappingReviewData):
        return {
            "card_id": data.card_id,
            "target_region": data.target_region,
            "state": data.state,
            "as_of": data.as_of,
            "coverage_scope": data.coverage_scope,
            "source_id": source_id,
        }
    if isinstance(data, RelatedData):
        return {
            "id": data.id,
            "from_card_id": data.from_card_id,
            "to_card_id": data.to_card_id,
            "relation": data.relation,
            "source_kind": data.source_kind,
            "source_id": source_id,
        }
    raise ValueError("Unsupported identity projection")


def _printing(
    db: Database,
    plan: PreviewPlan,
    record: RegistryRecord,
    arts: dict[tuple[str, str], str],
) -> None:
    data = record.data
    if not isinstance(data, PrintingData):
        raise TypeError("Expected printing record")
    evidence = plan.evidence[data.region, data.card_no]
    rarities = {face.rarity_raw for face in evidence.faces}
    if len(rarities) != 1:
        raise ValueError("Printing faces disagree on rarity; review required")
    rarity = evidence.faces[0].rarity_raw
    db.insert(
        "printing",
        {
            "id": data.id,
            "card_id": data.card_id,
            "region": data.region,
            "card_no": data.card_no,
            "card_no_state": "official",
            "catalog_state": "official",
            "decklog_available": True,
            "decklog_verification": "unverified",
            "decklog_source_id": evidence.source.id,
            "variant_key": data.variant_key,
            "home_set_id": data.home_set_id,
            "rarity_raw": rarity,
            "premium": True if rarity == "プレミアム" else None,
            "source_id": evidence.source.id,
        },
    )
    for mapping in data.source_face_map:
        db.insert(
            "printing_face",
            {
                "printing_id": data.id,
                "face_id": mapping.face_id,
                "card_id": data.card_id,
                "art_id": arts.get((data.id, mapping.face_id)),
                "embellishment_state": "unreviewed",
                "printed_text_state": "unknown",
                "credit_raw": evidence.faces[mapping.source_index].credit_raw,
                "source_id": evidence.source.id,
            },
        )
