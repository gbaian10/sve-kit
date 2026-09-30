"""Confirmed family projection and atomic composition with identity staging."""

import hashlib
import re
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.products.evidence import resolve_evidence
from sve_carddb.products.models import FamilyRecord, Lang, LocalizedText
from sve_carddb.registry.inputs import digest
from sve_carddb.registry.preview import populate_preview
from sve_carddb.registry.records import RecordData

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build_db import Value
    from sve_carddb.build_db.database import Database
    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.registry.preview import PreviewPlan


class Language(RecordData):
    code: Lang
    fallback_order: tuple[Lang, ...]
    display_name: str


def populate_families(
    db: Database,
    catalog: ProductSnapshot,
    *,
    authored_revision: str,
    languages: tuple[Language, ...] = (),
    stores: Mapping[str, Path] | None = None,
) -> None:
    """Project inside the caller's transaction; unsupported record kinds fail explicitly."""
    if not re.fullmatch(r"[0-9a-f]{40}", authored_revision):
        raise ValueError("Authored revision must be a full Git commit SHA")
    unsupported = sorted(
        {
            record.kind
            for record in catalog.records.values()
            if not isinstance(record, FamilyRecord)
        }
    )
    if unsupported:
        raise ValueError(
            "Product/inclusion DB projection is not implemented: "
            + ", ".join(unsupported)
        )
    evidence = resolve_evidence(
        tuple(ref for record in catalog.records.values() for ref in record.evidence),
        {} if stores is None else stores,
    )
    _languages(db, languages)
    texts = _Texts(db)
    for record in catalog.records.values():
        if (
            isinstance(record, FamilyRecord)
            and record.data.name.lang not in texts.languages
        ):
            raise ValueError("Product text language is not registered")
    links: set[tuple[str, str, str]] = set()
    for shard in catalog.shards:
        source_id = "authored:v1:" + digest(
            {
                "path": shard.path,
                "hash": shard.content_hash,
                "revision": authored_revision,
            }
        ).removeprefix("sha256:")
        db.insert(
            "source_record",
            {
                "id": source_id,
                "kind": "authored",
                "sha256": shard.content_hash,
                "authored_path": "authored/" + shard.path,
                "authored_revision": authored_revision,
                "parser_version": "product-authored-v1",
            },
        )
        decision = shard.envelope.decisions[0]
        values = decision.model_dump(
            mode="json", exclude={"members", "reviewed_precision"}
        )
        # DB has no precision/member columns; the immutable envelope retains both.
        row: dict[str, Value] = {
            key: value
            for key, value in values.items()
            if isinstance(value, str) or value is None
        }
        row["sample_ids"] = Json(list[JsonValue](decision.sample_ids))
        db.insert("decision", row)
        db.insert(
            "decision_source",
            {
                "decision_id": decision.id,
                "source_id": source_id,
                "role": "product_envelope",
                "locator": shard.path,
            },
        )
        for record in shard.envelope.records:
            for ref in record.evidence:
                checked = evidence[ref]
                _source(db, checked.source.model_dump() | {"kind": checked.kind})
                role = "product_evidence:" + digest(
                    ref.model_dump(mode="json")
                ).removeprefix("sha256:")
                key = decision.id, checked.source.id, role
                if key not in links:
                    db.insert(
                        "decision_source",
                        {
                            "decision_id": decision.id,
                            "source_id": checked.source.id,
                            "role": role,
                            "locator": ref.locator,
                        },
                    )
                    links.add(key)
            if decision.state == "confirmed" and isinstance(record, FamilyRecord):
                data = record.data
                db.insert(
                    "product_family",
                    {
                        "id": data.id,
                        "code": data.code,
                        "public_code": data.public_code,
                        "kind": data.kind,
                        "name_unit_id": texts.intern(data.name),
                        "decision_id": decision.id,
                    },
                )


def populate_product_preview(
    db: Database,
    catalog: ProductSnapshot,
    plan: PreviewPlan,
    *,
    authored_revision: str,
    languages: tuple[Language, ...] = (),
    stores: Mapping[str, Path] | None = None,
) -> None:
    """Compose family and identity writes inside one rebuild/caller transaction."""
    if catalog.registry_index_content != plan.snapshot.files.index_content:
        raise ValueError("Product catalog and preview must use the same registry input")
    populate_families(
        db,
        catalog,
        authored_revision=authored_revision,
        languages=languages,
        stores=stores,
    )
    populate_preview(db, plan, authored_revision=authored_revision)


def import_product_preview(
    db: Database,
    catalog: ProductSnapshot,
    plan: PreviewPlan,
    *,
    authored_revision: str,
    languages: tuple[Language, ...] = (),
    stores: Mapping[str, Path] | None = None,
) -> None:
    """Own one transaction for the complete family/audit/identity graph."""
    with db.transaction():
        populate_product_preview(
            db,
            catalog,
            plan,
            authored_revision=authored_revision,
            languages=languages,
            stores=stores,
        )


def _languages(db: Database, languages: tuple[Language, ...]) -> None:
    existing = {row.values["code"]: row.values for row in db.rows("language")}
    for language in languages:
        values: dict[str, Value] = {
            "code": language.code,
            "fallback_order": Json(list[JsonValue](language.fallback_order)),
            "display_name": language.display_name,
        }
        if language.code in existing:
            if existing[language.code] != values:
                raise ValueError("Conflicting language configuration")
        else:
            db.insert("language", values)
            existing[language.code] = values


def _source(db: Database, values: dict[str, Value]) -> None:
    previous = next(
        (
            row.values
            for row in db.rows("source_record")
            if row.values["id"] == values["id"]
        ),
        None,
    )
    if previous is None:
        db.insert("source_record", values)
    elif any(previous[key] != value for key, value in values.items()):
        raise ValueError("Conflicting product evidence source metadata")


class _Texts:
    def __init__(self, db: Database) -> None:
        self.db = db
        self.languages = {row.values["code"] for row in db.rows("language")}
        self.ids = {row.values["id"]: row.values for row in db.rows("text_unit")}
        self.hashes = {
            (row["lang"], row["content_hash"]): row for row in self.ids.values()
        }

    def intern(self, text: LocalizedText) -> str:
        if text.lang not in self.languages:
            raise ValueError("Product text language is not registered")
        checksum = hashlib.sha256(text.text.encode("utf-8")).hexdigest()
        text_id = f"t:{text.lang}:{checksum[:16]}"
        content_hash = "sha256:" + checksum
        values: dict[str, Value] = {
            "id": text_id,
            "lang": text.lang,
            "text": text.text,
            "content_hash": content_hash,
        }
        for previous in (
            self.ids.get(text_id),
            self.hashes.get((text.lang, content_hash)),
        ):
            if previous is not None and previous != values:
                raise ValueError(
                    "Text unit hash/ID collision or inconsistent exact bytes"
                )
        if text_id not in self.ids:
            self.db.insert("text_unit", values)
            self.ids[text_id] = values
            self.hashes[text.lang, content_hash] = values
        return text_id
