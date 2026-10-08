"""Whole-field machine translations of one owner, written without any template layer."""

from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.build.rows import insert_exact
from sve_carddb.build.t2_translation import OWNERS
from sve_carddb.core.json import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value

_OWNER_COLUMNS = frozenset(name for group in OWNERS for name in group)


def write(  # ruff: ignore[too-many-arguments] -- owner, field, source and quality are independent inputs of one row set
    db: Database,
    owner: dict[str, str],
    *,
    field: str,
    lang: str,
    source_unit_id: str,
    text: str,
    origin: Literal["project", "machine"],
    low_confidence: bool,
) -> None:
    """Insert context, use, translation and selection; a differing repeat is a conflict."""
    if not owner or not set(owner) <= _OWNER_COLUMNS:
        raise ValueError("Direct translation owner must use known owner columns")
    units = db.select(
        "text_unit", db.columns("text_unit"), where={"id": source_unit_id}
    )
    if len(units) != 1:
        raise ValueError("Direct translation source text unit is absent")
    context = (
        "ctx:"
        + digest(
            canonical(
                {
                    "recipe": "context-v1",
                    "source_unit_id": source_unit_id,
                    "semantic_variant": "default",
                }
            )
        )[7:]
    )
    insert_exact(
        db,
        "translation_context",
        {
            "id": context,
            "source_unit_id": source_unit_id,
            "semantic_variant": "default",
        },
        ("id",),
    )
    use = (
        "use:"
        + digest(
            canonical(
                {
                    "recipe": "use-v1",
                    "owner": dict[str, JsonValue](owner),
                    "context_id": context,
                    "field": field,
                    "ordinal": None,
                }
            )
        )[7:]
    )
    values: dict[str, Value] = dict.fromkeys(_OWNER_COLUMNS)
    values.update(owner)
    values.update(id=use, context_id=context, field=field, ordinal=None)
    insert_exact(db, "translation_use", values, ("id",))
    checksum = digest(
        canonical(
            {
                "recipe": "direct-v1",
                "context_id": context,
                "target_lang": lang,
                "text": text,
                "origin": origin,
                "low_confidence": low_confidence,
            }
        )
    )[7:]
    identifier = "tr:" + checksum
    insert_exact(
        db,
        "translation",
        {
            "id": identifier,
            "context_id": context,
            "target_lang": lang,
            "revision": int(checksum[:13], 16),
            "text": text,
            "tokens": None,
            "origin": origin,
            "authority": "unofficial",
            "low_confidence": low_confidence,
            "source_hash": units[0].values["content_hash"],
            "source_id": None,
        },
        ("id",),
    )
    chosen = db.select(
        "translation_selection",
        db.columns("translation_selection"),
        where={"context_id": context, "target_lang": lang},
    )
    if chosen and chosen[0].values["translation_id"] != identifier:
        raise ValueError("One source text cannot have two direct translations")
    insert_exact(
        db,
        "translation_selection",
        {
            "context_id": context,
            "target_lang": lang,
            "translation_id": identifier,
        },
        ("context_id", "target_lang"),
    )
