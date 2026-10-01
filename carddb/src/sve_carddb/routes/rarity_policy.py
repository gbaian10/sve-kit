"""User-approved exact rarity classification; this does not adopt processing facts."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.routes.defaults import GeneralEvidence
from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.build_db import Database

_GENERAL = {
    "jp": ("BR", "GR", "LG", "SR"),
    "en": ("Bronze", "Gold", "Legendary", "Silver"),
}
_EXCLUDED = {
    "jp": (
        "BR・プレミアム",
        "GR・プレミアム",
        "SL",
        "SP",
        "SR・プレミアム",
        "SSP",
        "UR",
        "プレミアム",
    ),
    "en": (
        "Bronze / Premium",
        "Gold / Premium",
        "Premium",
        "Silver / Premium",
        "Special",
        "Super Legendary",
        "Super Special",
        "Ultimate",
    ),
}
_NON_GENERAL = {"jp": ("-", "PR"), "en": ("-", "Promo")}


@dataclass(frozen=True)
class RarityWhitelist:
    jp: tuple[str, ...]
    en: tuple[str, ...]

    def __post_init__(self) -> None:
        """The input exposes an approved policy, without giving callers new policy authority."""
        if self.jp != _GENERAL["jp"] or self.en != _GENERAL["en"]:
            raise ValueError(
                "General rarity whitelist differs from user-approved policy"
            )

    @staticmethod
    def configuration() -> dict[str, JsonValue]:
        """Pin all approved labels separately from adopted vocabulary codes."""
        value: dict[str, JsonValue] = {
            "policy": "general-rarity-approved-2026-10-01-v1",
            "general": {
                region: list[JsonValue](raw) for region, raw in _GENERAL.items()
            },
            "excluded": {
                region: list[JsonValue](raw) for region, raw in _EXCLUDED.items()
            },
            "non_general": {
                region: list[JsonValue](raw) for region, raw in _NON_GENERAL.items()
            },
        }
        return {
            "general_rarity_policy": value,
            "general_rarity_policy_hash": digest(canonical(value)),
        }

    @staticmethod
    def classify(region: str, raw: str | None) -> bool | None:
        """Keep unknown labels unknown; premium takes priority over a base rarity."""
        if region not in _GENERAL:
            raise ValueError("Unsupported rarity region")
        if raw is None:
            return None
        if "プレミアム" in raw or "Premium" in raw:
            return False
        if raw in _GENERAL[region]:
            return True
        if raw in _EXCLUDED[region] or raw in _NON_GENERAL[region]:
            return False
        return None

    def evidence(self, db: Database) -> dict[str, GeneralEvidence]:
        """Classify source raw labels while frame and stamp remain unknown."""
        evidence = {}
        for row in db.rows("printing"):
            raw = row.values["rarity_raw"]
            if raw is not None and not isinstance(raw, str):
                raise ValueError("Printing rarity raw must be text or unknown")
            evidence[str(row.values["id"])] = GeneralEvidence(
                general_rarity=self.classify(str(row.values["region"]), raw),
            )
        return evidence


APPROVED_GENERAL_RARITIES = RarityWhitelist(jp=_GENERAL["jp"], en=_GENERAL["en"])
