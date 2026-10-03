"""Materialize owner-checked names with render-v1, without global official selection."""

from typing import TYPE_CHECKING

from sve_carddb.build_db.rows import insert_exact
from sve_carddb.build_inputs import insert_raw_sources
from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build_db import Database, Value
    from sve_carddb.translations.name_build import NameSource
    from sve_carddb.translations.name_selection import NameCandidate


def materialize_name(
    db: Database, source: NameSource, context_id: str, candidate: NameCandidate
) -> str:
    """Evidence changes remain private; identical ordinary names keep their public ID."""
    if candidate.decision_id is None or not candidate.reviewed_at:
        raise ValueError("Selected name lacks an adopted decision and review date")
    if candidate.authority == "digital_official" and candidate.source is None:
        raise ValueError("Selected official name lacks frozen source evidence")
    dependency: dict[str, JsonValue] = {
        "recipe": "owner-name-v1",
        "source_name_hash": source.source_hash,
        "target_name_hash": digest(candidate.text.encode()),
    }
    if candidate.authority == "digital_official":
        dependency["game"] = candidate.origin.removeprefix("official_")
    checksum = digest(
        canonical(
            {
                "recipe": "render-v1",
                "context_id": context_id,
                "target_lang": "zh-Hant",
                "dependency_key": dependency,
                "text": candidate.text,
                "origin": candidate.origin,
                "authority": candidate.authority,
            }
        )
    )[7:]
    if candidate.source is not None:
        insert_raw_sources(db, (candidate.source,))
    values: dict[str, Value] = {
        "id": "tr:" + checksum,
        "context_id": context_id,
        "target_lang": "zh-Hant",
        "revision": int(checksum[:13], 16),
        "text": candidate.text,
        "tokens": None,
        "origin": candidate.origin,
        "authority": candidate.authority,
        "status": "reviewed",
        "source_hash": source.source_hash,
        "source_id": None if candidate.source is None else candidate.source.id,
        "translated_by": "owner-name-v1",
        "translated_at": candidate.reviewed_at,
        "decision_id": candidate.decision_id,
    }
    existing = db.select(
        "translation", db.columns("translation"), where={"id": values["id"]}
    )
    if existing:
        # Different verified owners can provide the same wording through distinct receipts.
        # Their full evidence stays in F1; the first sorted owner supplies private row metadata.
        ignored = {"source_id", "translated_by", "translated_at", "decision_id"}
        if any(
            existing[0].values[key] != value
            for key, value in values.items()
            if key not in ignored
        ):
            raise ValueError("Stable name translation ID collision")
    else:
        insert_exact(db, "translation", values, ("id",))
    return str(values["id"])
