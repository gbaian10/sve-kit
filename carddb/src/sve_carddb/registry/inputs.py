"""Typed local input boundaries and cross-region consistency checks."""

import csv
import hashlib
import json
from collections import defaultdict
from typing import TYPE_CHECKING, Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, TypeAdapter

if TYPE_CHECKING:
    from pathlib import Path

JSON_VALUE: TypeAdapter[JsonValue] = TypeAdapter(JsonValue)
CLASSES = {
    "エルフ": "Forestcraft",
    "ロイヤル": "Swordcraft",
    "ウィッチ": "Runecraft",
    "ドラゴン": "Dragoncraft",
    "ナイトメア": "Abysscraft",
    "ビショップ": "Havencraft",
    "ニュートラル": "Neutral",
    "-": "-",
}
TYPES = {
    "フォロワー": "Follower",
    "スペル": "Spell",
    "アミュレット": "Amulet",
    "リーダー": "Leader",
    "EP": "Evolution Point",
    "SEP": "Super-Evolution Point",
    "イクイップメント": "Equipment",
    "クレスト": "Crest",
    "エボルヴ": "Evolved",
    "アドバンス": "Advanced",
    "トークン": "Token",
}


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


class Target(BaseModel):
    jp_no: str


class Candidate(BaseModel):
    en_no: str
    category: Literal["A", "B", "C"]
    jp_candidates: list[Target]


class Confirmation(BaseModel):
    model_config = ConfigDict(extra="forbid")
    en_no: str
    jp_no: str
    verdict: Literal[
        "confirmed",
        "corrected_by_art",
        "confirmed_en_only",
        "confirmed_en_original_art",
        "en_only_rules_differ",
        "en_only_class_differs",
        "en_only_reskin_same_rules",
    ]
    confirmed_on: str


class Mapping(BaseModel):
    targets: dict[str, str | None]
    original_art: set[str]
    reskins: dict[str, str]


def read_cards(path: Path) -> dict[str, Card]:
    """Reject repeated card numbers before any dictionary can overwrite them."""
    result: dict[str, Card] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        card = Card.model_validate_json(line)
        if card.number in result:
            raise ValueError(f"Duplicate card number: {card.number}")
        result[card.number] = card
    return result


def read_mapping(candidates: Path, confirmations: Path) -> Mapping:
    """Apply ordered human overrides, including revocation of original-art claims."""
    result = Mapping(targets={}, original_art=set(), reskins={})
    for line in candidates.read_text(encoding="utf-8").splitlines():
        candidate = Candidate.model_validate_json(line)
        if candidate.en_no in result.targets:
            raise ValueError(f"Duplicate candidate: {candidate.en_no}")
        if candidate.category != "C" and not candidate.jp_candidates:
            raise ValueError(f"Missing candidate target: {candidate.en_no}")
        result.targets[candidate.en_no] = (
            None if candidate.category == "C" else candidate.jp_candidates[0].jp_no
        )
    lines = confirmations.read_text(encoding="utf-8").splitlines()
    for raw in csv.DictReader(
        (line for line in lines if not line.startswith("#")), delimiter="\t"
    ):
        row = Confirmation.model_validate(raw)
        if row.en_no not in result.targets:
            raise ValueError(f"Unknown confirmation: {row.en_no}")
        _apply(result, row)
    return result


def _apply(result: Mapping, row: Confirmation) -> None:
    if row.verdict == "confirmed_en_original_art":
        result.original_art.add(row.en_no)
    elif row.verdict in {"confirmed", "corrected_by_art"}:
        if not row.jp_no:
            raise ValueError(f"Missing reviewed target: {row.en_no}")
        result.targets[row.en_no] = row.jp_no
        result.reskins.pop(row.en_no, None)
    else:
        result.targets[row.en_no] = None
        result.original_art.discard(row.en_no)
        result.reskins.pop(row.en_no, None)
        if row.verdict == "en_only_reskin_same_rules":
            result.reskins[row.en_no] = row.jp_no


def _validate_faces(source: Card, target: Card) -> None:
    if len(source.faces) != len(target.faces):
        raise ValueError(f"Face count mismatch: {source.number} -> {target.number}")
    for english, japanese in zip(source.faces, target.faces, strict=True):
        expected_type = " / ".join(
            TYPES[part] for part in japanese.card_type.split("・")
        )
        if english.info["Class"] != CLASSES[japanese.card_class]:
            raise ValueError(f"Class mismatch: {source.number} -> {target.number}")
        if english.info["Card Type"] != expected_type:
            raise ValueError(f"Type mismatch: {source.number} -> {target.number}")
        for key in ("cost", "power", "hp"):
            if english.stats[key] != getattr(japanese, key):
                raise ValueError(f"{key} mismatch: {source.number} -> {target.number}")


def validate_mapping(
    jp: dict[str, Card], en: dict[str, Card], mapping: Mapping
) -> None:
    """Fail on coverage, class, type, numeric or same-card English name conflicts."""
    if set(en) != set(mapping.targets):
        raise ValueError("Candidate and English source coverage differ")
    groups: dict[str, list[str]] = defaultdict(list)
    for number, target in sorted(mapping.targets.items()):
        if target is not None:
            _validate_faces(en[number], jp[target])
            groups[jp[target].structure_hash("jp")].append(number)
    for number, target in mapping.reskins.items():
        _validate_faces(en[number], jp[target])
    for numbers in groups.values():
        names = {tuple(face.name for face in en[number].faces) for number in numbers}
        if len(names) != 1:
            raise ValueError(f"English name mismatch: {numbers}")
