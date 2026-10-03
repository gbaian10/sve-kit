"""Frozen physical observation boundary used by reviewed identity evidence."""

import hashlib
import json
import re
from dataclasses import asdict
from typing import TYPE_CHECKING

from pydantic import BaseModel, Field, JsonValue

if TYPE_CHECKING:
    from sve_carddb.template_semantics.v1 import official_jp


def digest(value: JsonValue) -> str:
    """Hash canonical UTF-8 JSON without Unicode or whitespace normalization."""
    return "sha256:" + hashlib.sha256(canonical(value)).hexdigest()


def canonical(value: JsonValue) -> bytes:
    """Encode the registry's version-one canonical JSON recipe."""
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode()


class Face(BaseModel):
    name: str
    card_class: str = ""
    card_type: str = ""
    traits: list[str] = Field(default_factory=list)
    cost: str = "-"
    power: str = "-"
    hp: str = "-"
    text: str | None = None
    sections: list[str] = Field(default_factory=list)
    info: dict[str, str] = Field(default_factory=dict)
    stats: dict[str, str] = Field(default_factory=dict)
    speech: str | None = None
    image: str

    def structure(self, region: str) -> dict[str, JsonValue]:
        """Expose all identity-bearing fields except reviewed wording variants."""
        if region == "jp":
            return {
                "name": self.name,
                "class": self.card_class,
                "type": self.card_type,
                "traits": list[JsonValue](sorted(self.traits)),
                "stats": [self.cost, self.power, self.hp],
            }
        return {
            "name": self.name,
            "class": self.info["Class"],
            "type": self.info["Card Type"],
            "traits": list[JsonValue](sorted(self.info.get("Trait", "-").split(" / "))),
            "stats": [self.stats[key] for key in ("cost", "power", "hp")],
        }


class Card(BaseModel):
    number: str
    faces: list[Face] = Field(min_length=1, max_length=2)

    def structure_hash(self, region: str) -> str:
        """Identify reviewed grouping candidates, never public card IDs."""
        return digest([face.structure(region) for face in self.faces])

    def rules_hash(self) -> str:
        """Pin every face and auxiliary section without claiming equivalence."""
        return digest(
            [
                face.model_dump(include={"name", "text", "speech", "sections"})
                for face in self.faces
            ]
        )


def legacy_jp(record: official_jp.CardRecord) -> Card:
    """Retain the reviewed original tokenizer independently of current features."""
    card = Card.model_validate(asdict(record))
    for face, raw in zip(card.faces, record.faces, strict=True):
        face.traits = (
            []
            if raw.trait_raw == "-"
            else re.findall(r"ジオ・テオゴニア|[^・]+", raw.trait_raw)
        )
    return card


def observation(card: Card, region: str) -> dict[str, JsonValue]:
    """Pin all identity-bearing bytes with the reviewed registry recipe."""
    return {
        "region": region,
        "card_no": card.number,
        "observation_hash": digest(card.model_dump(mode="json")),
        "rules_hash": card.rules_hash(),
        "recipe": "registry-observation-v1",
    }
