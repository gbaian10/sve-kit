"""Shared JP/EN rarity codes while preserving source labels and unknown evidence."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build import Database

_LABELS = {
    "jp": {
        "BR": "br",
        "SR": "sr",
        "GR": "gr",
        "LG": "lg",
        "SL": "sl",
        "UR": "ur",
        "SP": "sp",
        "SSP": "ssp",
        "PR": "pr",
        "-": "unknown",
        "プレミアム": "premium",
    },
    "en": {
        "Bronze": "br",
        "Silver": "sr",
        "Gold": "gr",
        "Legendary": "lg",
        "Super Legendary": "sl",
        "Ultimate": "ur",
        "Special": "sp",
        "Super Special": "ssp",
        "Promo": "pr",
        "-": "unknown",
        "Premium": "premium",
    },
}


def normalize(region: str, raw: str) -> tuple[str | None, bool | None]:
    """An unrecognized spelling stays unknown rather than acquiring a guessed tier."""
    if region not in _LABELS:
        raise ValueError("Unsupported rarity region")
    suffix = "・プレミアム" if region == "jp" else " / Premium"
    premium = raw.endswith(suffix)
    label = raw.removesuffix(suffix)
    code = _LABELS[region].get(label)
    if code is None or (premium and code in {"unknown", "premium"}):
        return None, None
    return code, premium or code == "premium"


def populate(db: Database) -> list[dict[str, JsonValue]]:
    """Normalize every staged printing and report each unrecognized source value."""
    from sve_carddb.domains.products.models import LocalizedText  # ruff: ignore[import-outside-top-level] -- product staging imports registry preview
    from sve_carddb.domains.text_observations.intern import TextInterner  # ruff: ignore[import-outside-top-level] -- text composition imports registry preview

    texts = TextInterner(db)
    registered = {
        str(row.values["code"])
        for row in db.rows("vocabulary")
        if row.values["kind"] == "rarity"
    }
    diagnostics: list[dict[str, JsonValue]] = []
    for row in db.rows("printing"):
        values = row.values
        region = str(values["region"])
        raw = str(values["rarity_raw"])
        code, premium = normalize(region, raw)
        if code is None:
            diagnostics.append(
                {
                    "printing_id": str(values["id"]),
                    "region": region,
                    "rarity_raw": raw,
                    "reason": "unrecognized_rarity",
                }
            )
        elif code not in registered:
            db.insert(
                "vocabulary",
                {
                    "kind": "rarity",
                    "code": code,
                    "active": True,
                    "label_unit_id": texts.intern(
                        LocalizedText(
                            lang="ja" if region == "jp" else "en", text=code.upper()
                        )
                    ),
                },
            )
            registered.add(code)
        db.update(
            "printing", {"id": values["id"]}, {"rarity_code": code, "premium": premium}
        )
    return diagnostics
