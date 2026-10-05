"""Confirmed family projection and atomic composition with identity staging."""

import hashlib
import re
from typing import TYPE_CHECKING

from sve_carddb.build_inputs import (
    BuildContext,
    InputRecord,
    SourceUse,
    input_record,
    insert_raw_sources,
)
from sve_carddb.catalog.languages import register_languages
from sve_carddb.products.evidence import CheckedSource, resolve_evidence
from sve_carddb.products.models import Evidence, FamilyRecord, Language, LocalizedText
from sve_carddb.products.official_importer import populate_official_products
from sve_carddb.registry.inputs import digest
from sve_carddb.registry.preview import populate_preview
from sve_carddb.snapshot.values import canonical

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build_db import Value
    from sve_carddb.build_db.database import Database
    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.products.plan import OfficialProducts
    from sve_carddb.registry.preview import PreviewPlan


def _require_family_catalog(catalog: ProductSnapshot) -> None:
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


def populate_families(
    db: Database,
    catalog: ProductSnapshot,
    *,
    authored_revision: str,
    build: BuildContext,
    languages: tuple[Language, ...] = (),
    stores: Mapping[str, Path] | None = None,
) -> InputRecord:
    """Project inside the caller's transaction; unsupported record kinds fail explicitly."""
    if not re.fullmatch(r"[0-9a-f]{40}", authored_revision):
        raise ValueError("Authored revision must be a full Git commit SHA")
    _require_family_catalog(catalog)
    evidence = resolve_evidence(
        tuple(ref for record in catalog.records.values() for ref in record.evidence),
        {} if stores is None else stores,
    )
    uses: list[SourceUse] = []
    insert_raw_sources(db, (checked.source for checked in evidence.values()))
    register_languages(db, languages)
    texts = _Texts(db)
    for record in catalog.records.values():
        if (
            isinstance(record, FamilyRecord)
            and record.data.name.lang not in texts.languages
        ):
            raise ValueError("Product text language is not registered")
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
        for record in shard.envelope.records:
            uses.extend(_product_use(ref, evidence[ref]) for ref in record.evidence)
            if record.state == "confirmed" and isinstance(record, FamilyRecord):
                data = record.data
                db.insert(
                    "product_family",
                    {
                        "id": data.id,
                        "code": data.code,
                        "public_code": data.public_code,
                        "kind": data.kind,
                        "name_unit_id": texts.intern(data.name),
                    },
                )

    inputs = input_record(build, uses)
    inputs.verify(db, build, product_source_uses(catalog, evidence), complete=False)
    return inputs


def _product_use(reference: Evidence, checked: CheckedSource) -> SourceUse:
    return SourceUse(
        source=checked.source,
        usage="product_evidence_closure",
        locator=canonical(reference.model_dump(mode="json")).decode(),
    )


def product_source_uses(
    catalog: ProductSnapshot, evidence: Mapping[Evidence, CheckedSource]
) -> tuple[SourceUse, ...]:
    """Declare every evidence use from the complete validated product input."""
    return tuple(
        _product_use(ref, evidence[ref])
        for record in catalog.records.values()
        for ref in record.evidence
    )


def product_preview_uses(
    catalog: ProductSnapshot,
    plan: PreviewPlan,
    stores: Mapping[str, Path],
    *,
    official: OfficialProducts | None = None,
) -> tuple[SourceUse, ...]:
    """Declare the complete expected source closure independently of database writes."""
    _require_family_catalog(catalog)
    evidence = resolve_evidence(
        tuple(ref for record in catalog.records.values() for ref in record.evidence),
        stores,
    )
    if official is not None and official.preview != plan:
        raise ValueError(
            "Official products and identity must use the same preview plan"
        )
    if official is not None and official.identities.catalog != catalog:
        raise ValueError("Official product types and families require the same catalog")
    return (
        *product_source_uses(catalog, evidence),
        *plan.source_uses(),
        *(() if official is None else official.source_uses()),
    )


def populate_product_preview(  # ruff: ignore[too-many-arguments] -- compose explicit build, authored, language and archive inputs
    db: Database,
    catalog: ProductSnapshot,
    plan: PreviewPlan,
    *,
    authored_revision: str,
    build: BuildContext,
    languages: tuple[Language, ...] = (),
    stores: Mapping[str, Path] | None = None,
    official: OfficialProducts | None = None,
) -> InputRecord:
    """Compose family and identity writes inside one rebuild/caller transaction."""
    if catalog.registry_index_content != plan.snapshot.files.index_content:
        raise ValueError("Product catalog and preview must use the same registry input")
    if official is not None and official.identities.revision != authored_revision:
        raise ValueError("Product identity authored revision mismatch")
    expected = product_preview_uses(
        catalog, plan, {} if stores is None else stores, official=official
    )
    family_inputs = populate_families(
        db,
        catalog,
        authored_revision=authored_revision,
        build=build,
        languages=languages,
        stores=stores,
    )
    identity_inputs = populate_preview(
        db, plan, authored_revision=authored_revision, build=build
    )
    official_inputs = (
        ()
        if official is None
        else populate_official_products(
            db, official, build=build, texts=_Texts(db)
        ).uses
    )
    inputs = input_record(
        build, (*family_inputs.uses, *identity_inputs.uses, *official_inputs)
    )
    inputs.verify(db, build, expected)
    return inputs


def import_product_preview(  # ruff: ignore[too-many-arguments] -- transaction owner forwards the complete pinned inputs
    db: Database,
    catalog: ProductSnapshot,
    plan: PreviewPlan,
    *,
    authored_revision: str,
    build: BuildContext,
    languages: tuple[Language, ...] = (),
    stores: Mapping[str, Path] | None = None,
    official: OfficialProducts | None = None,
) -> InputRecord:
    """Own one transaction for the complete family/audit/identity graph."""
    with db.transaction():
        return populate_product_preview(
            db,
            catalog,
            plan,
            authored_revision=authored_revision,
            build=build,
            languages=languages,
            stores=stores,
            official=official,
        )


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
