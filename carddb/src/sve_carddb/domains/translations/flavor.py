"""Flavor text is translated whole, keyed by its exact source hash; no templates or parameters."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Annotated, Literal, Self

from pydantic import Field, ValidationError, field_validator, model_validator

from sve_carddb.core.json import canonical
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.domains.translations.direct import write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database

DIRECTORY = "flavor-translations"
type Key = tuple[str, str]
LANGS = ("zh-Hant",)


class Entry(RecordData):
    source_hash: Hash
    lang: Literal["zh-Hant"]
    text: Text
    origin: Literal["project", "machine"]
    low_confidence: bool

    @field_validator("text")
    @classmethod
    def _display_ready(cls, value: str) -> str:
        """The text is shown as written, so reject what a renderer would have to trim."""
        if "\r" in value or value != value.strip():
            raise ValueError("Flavor translation must be LF text without outer space")
        if any(line != line.rstrip() for line in value.split("\n")):
            raise ValueError("Flavor translation lines cannot end in whitespace")
        return value


class Shard(RecordData):
    kind: Literal["flavor_translation_shard"]
    entries: Annotated[tuple[Entry, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        keys = [(e.source_hash, e.lang) for e in self.entries]
        if keys != sorted(set(keys)):
            raise ValueError("Flavor entries must be sorted and unique")
        return self


@dataclass(frozen=True)
class Report:
    entries: int
    applied: int
    unused: int

    def payload(self) -> dict[str, int]:
        """Counts for the build report; unused entries show owners outside the publication."""
        return {"entries": self.entries, "applied": self.applied, "unused": self.unused}


def load(authored: Path) -> dict[Key, Entry]:
    """Read every shard of the directory; an absent directory means no translations yet."""
    root = authored / DIRECTORY
    if not root.exists():
        return {}
    if root.is_symlink() or not root.is_dir():
        raise ValueError("Flavor translation input must be a plain directory")
    result: dict[Key, Entry] = {}
    for path in sorted(root.iterdir()):
        if path.suffix != ".yaml" or path.is_symlink() or not path.is_file():
            raise ValueError("Flavor translation directory holds only YAML files")
        try:
            shard = Shard.model_validate_json(canonical(read_yaml(path)))
        except ValidationError:
            raise ValueError(f"Invalid flavor translation shard {path.name}") from None
        for entry in shard.entries:
            key = (entry.source_hash, entry.lang)
            if key in result:
                raise ValueError("Flavor source text appears in two shards")
            result[key] = entry
    return result


def apply(db: Database, entries: dict[Key, Entry]) -> Report:
    """Translate every face whose flavor hash matches, whatever its printed-text state."""
    applied = 0
    used: set[Key] = set()
    cards = {
        row.values["id"]
        for row in db.select("card", db.columns("card"))
        if row.values["identity_state"] == "confirmed"
    }
    for row in db.rows("printing_face"):
        values = row.values
        unit = values["flavor_unit_id"]
        if unit is None or values["card_id"] not in cards:
            continue
        source = db.select("text_unit", db.columns("text_unit"), where={"id": unit})
        if len(source) != 1:
            raise ValueError("Flavor source text unit is absent")
        if source[0].values["lang"] != "ja":
            continue
        for lang in LANGS:
            key = (str(source[0].values["content_hash"]), lang)
            entry = entries.get(key)
            if entry is None:
                continue
            write(
                db,
                {
                    "printing_id": str(values["printing_id"]),
                    "face_id": str(values["face_id"]),
                },
                field="flavor",
                lang=lang,
                source_unit_id=str(unit),
                text=entry.text,
                origin=entry.origin,
                low_confidence=entry.low_confidence,
            )
            used.add(key)
            applied += 1
    return Report(len(entries), applied, len(entries) - len(used))
