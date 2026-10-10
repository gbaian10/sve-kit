"""Store versioned ruling documents and inactive reference records."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build import Json
from sve_carddb.build.rows import insert_exact
from sve_carddb.contracts.rulings import Resolution, RetainedReference
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData
from sve_carddb.domains.rulings.reader import document
from sve_carddb.domains.rulings.resolution import Report, build
from sve_carddb.domains.translations.four_layer_storage import payload

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.domains.rulings.reader import Document

PROFILE = "ruling-authored-v2"


def _parse[T: RecordData](model: type[T], data: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(data))
    except ValueError, TypeError:
        raise ValueError("Invalid stored ruling references") from None


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


def read(db: Database) -> Report:
    """Nullable SQL keys and JSON candidates cannot bypass logical validation."""
    for row in db.rows("ruling_document"):
        raw = payload(row)
        item = document(str(raw["source_path"]), str(raw["raw_text"]).encode())
        sources = db.select(
            "source_record",
            db.columns("source_record"),
            where={"id": str(raw["source_id"])},
        )
        if len(sources) != 1:
            raise ValueError("Missing exact ruling authored source")
        source = sources[0].values
        if (item.ruling.id, item.ruling.revision, item.source.sha256) != (
            raw["ruling_id"],
            raw["revision"],
            raw["source_hash"],
        ) or (
            source["kind"],
            source["sha256"],
            source["authored_path"],
            source["parser_version"],
        ) != ("authored", item.source.sha256, item.source.path, PROFILE):
            raise ValueError("Stored ruling version or exact source differs")
    resolutions = []
    retained = []
    for row in db.rows("ruling_resolution"):
        raw = payload(row)
        result = _parse(Resolution, raw["payload"])
        if (
            raw["id"]
            != "ruling-resolution:"
            + digest(canonical(result.model_dump(mode="json")))[7:]
            or raw["ruling_id"] != result.ruling_ref.id
            or raw["revision"] != result.ruling_ref.revision
            or raw["frame_id"]
            != (None if result.target is None else result.target.frame_id)
        ):
            raise ValueError(
                "Ruling resolution columns differ from their typed identity"
            )
        resolutions.append(result)
    for row in db.rows("ruling_retained_reference"):
        raw = payload(row)
        result_ir = _parse(RetainedReference, raw["payload"])
        if (raw["ruling_id"], raw["revision"], raw["reference_ordinal"]) != (
            result_ir.ruling_ref.id,
            result_ir.ruling_ref.revision,
            result_ir.ruling_ref.reference_ordinal,
        ):
            raise ValueError(
                "Retained ruling reference differs from its original ordinal"
            )
        retained.append(result_ir)
    return Report(tuple(resolutions), tuple(retained))


def require_active(
    _db: Database, identifiers: tuple[str, ...]
) -> tuple[Resolution, ...]:
    """N0 has no verified occurrence resolver; executable applicability waits for #499."""
    if identifiers:
        raise ValueError("Executable ruling dependency is missing or pending")
    return ()
