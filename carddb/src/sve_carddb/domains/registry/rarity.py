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
    },
}
# The approved catalog policy adopts no code for these: a bare premium has no base
# tier, and "-" is not a rarity, so only the premium flag can be known.
_UNCODED: dict[str, dict[str, bool | None]] = {
    "jp": {"-": None, "プレミアム": True},
    "en": {"-": None, "Premium": True},
}
_JP_LABELS = {code: label for label, code in _LABELS["jp"].items()}


def normalize(region: str, raw: str) -> tuple[str | None, bool | None] | None:
    """None marks an unrecognized spelling, which never acquires a guessed tier."""
    if region not in _LABELS:
        raise ValueError("Unsupported rarity region")
    if raw in _UNCODED[region]:
        return None, _UNCODED[region][raw]
    suffix = "・プレミアム" if region == "jp" else " / Premium"
    code = _LABELS[region].get(raw.removesuffix(suffix))
    return None if code is None else (code, raw.endswith(suffix))


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
        normalized = normalize(region, raw)
        if normalized is None:
            diagnostics.append(
                {
                    "printing_id": str(values["id"]),
                    "region": region,
                    "rarity_raw": raw,
                    "reason": "unrecognized_rarity",
                }
            )
        code, premium = normalized or (None, None)
        if code is not None and code not in registered:
            # The label stays the JP card-face label whichever region is staged first.
            db.insert(
                "vocabulary",
                {
                    "kind": "rarity",
                    "code": code,
                    "active": True,
                    "label_unit_id": texts.intern(
                        LocalizedText(lang="ja", text=_JP_LABELS[code])
                    ),
                },
            )
            registered.add(code)
        db.update(
            "printing", {"id": values["id"]}, {"rarity_code": code, "premium": premium}
        )
    return diagnostics
