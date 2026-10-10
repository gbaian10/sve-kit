"""Store versioned ruling documents and inactive reference records."""

from typing import TYPE_CHECKING

from sve_carddb.build import Json
from sve_carddb.build.rows import insert_exact
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.rulings.resolution import Report, build

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.domains.rulings.reader import Document

PROFILE = "ruling-authored-v2"


def write(db: Database, documents: tuple[Document, ...], revision: str) -> Report:
    """Store the current documents and their original applicability positions."""
    report = build(documents)
    for item in documents:
        source_id = (
            "authored:ruling:"
            + digest(canonical([revision, item.source.model_dump(mode="json")]))[7:]
        )
        insert_exact(
            db,
            "source_record",
            {
                "id": source_id,
                "kind": "authored",
                "sha256": item.source.sha256,
                "authored_path": item.source.path,
                "authored_revision": revision,
                "parser_version": PROFILE,
            },
            ("id",),
        )
        db.insert(
            "ruling_document",
            {
                "ruling_id": item.ruling.id,
                "revision": item.ruling.revision,
                "source_id": source_id,
                "source_path": item.source.path,
                "source_hash": item.source.sha256,
                "raw_text": item.raw.decode(),
            },
        )
    for resolution in report.resolutions:
        raw = resolution.model_dump(mode="json")
        db.insert(
            "ruling_resolution",
            {
                "id": "ruling-resolution:" + digest(canonical(raw))[7:],
                "ruling_id": resolution.ruling_ref.id,
                "revision": resolution.ruling_ref.revision,
                "frame_id": None
                if resolution.target is None
                else resolution.target.frame_id,
                "payload": Json(raw),
            },
        )
    for retained in report.retained:
        db.insert(
            "ruling_retained_reference",
            {
                "ruling_id": retained.ruling_ref.id,
                "revision": retained.ruling_ref.revision,
                "reference_ordinal": retained.ruling_ref.reference_ordinal,
                "payload": Json(retained.model_dump(mode="json")),
            },
        )
    return report
