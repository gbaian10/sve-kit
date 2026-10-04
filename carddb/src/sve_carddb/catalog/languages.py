"""Explicit UI language registration; card text never uses these fallbacks."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json

if TYPE_CHECKING:
    from sve_carddb.build_db import Database, Value
    from sve_carddb.products.models import Language


def register_languages(db: Database, languages: tuple[Language, ...]) -> None:
    """Reuse exact configuration and verify the complete fallback closure."""
    existing = {row.values["code"]: row.values for row in db.rows("language")}
    seen: set[str] = set()
    for language in languages:
        if language.code in seen:
            raise ValueError("Duplicate language configuration")
        seen.add(language.code)
        values: dict[str, Value] = {
            "code": language.code,
            "fallback_order": Json(list[JsonValue](language.fallback_order)),
            "display_name": language.display_name,
        }
        if language.code in existing:
            # Provenance belongs to the adoption; registration compares UI configuration.
            if any(
                existing[language.code][key] != value for key, value in values.items()
            ):
                raise ValueError("Conflicting language configuration")
        else:
            db.insert("language", values)
            existing[language.code] = values
    for registered in existing.values():
        fallback = registered["fallback_order"]
        assert isinstance(fallback, Json)
        assert isinstance(fallback.value, list)
        if registered["code"] in fallback.value or any(
            code not in existing for code in fallback.value
        ):
            raise ValueError("Invalid language fallback closure")
        if registered["code"] in {"ja", "en"} and "zh-Hant" in fallback.value:
            raise ValueError(
                "Japanese/English UI cannot fall back to Traditional Chinese"
            )
