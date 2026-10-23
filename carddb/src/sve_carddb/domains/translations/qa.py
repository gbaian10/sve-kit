"""Whole Japanese Q&A fields, keyed by their exact source text like flavor text."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Annotated, Literal, Self

from pydantic import Field, ValidationError, field_validator, model_validator

from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.domains.translations.direct import write
from sve_carddb.domains.translations.flavor import Entry as FlavorEntry

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database

DIRECTORY = "translations/qa"
FIELDS = ("question", "answer")


type Key = tuple[str, str, str, str]


class Entry(FlavorEntry):
    qa_id: Text
    field: Literal["question", "answer"]

    def key(self) -> Key:
        """Scope identical answers to their own Q&A while keeping exact source matching."""
        return self.qa_id, self.field, self.source_hash, self.lang


class Shard(RecordData):
    format: Literal[1]
    kind: Literal["qa_translation_shard"]
    entries: Annotated[tuple[Entry, ...], Field(min_length=1)]

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Authored format must be an integer")
        return value

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        keys = [e.key() for e in self.entries]
        if keys != sorted(set(keys)):
            raise ValueError("Q&A entries must be sorted and unique")
        return self


@dataclass(frozen=True)
class Report:
    entries: int
    applied: int
    stale: int

    def payload(self) -> dict[str, int]:
        """Unmatched source hashes are stale, including sources outside this build."""
        return {"entries": self.entries, "applied": self.applied, "stale": self.stale}


def load(authored: Path) -> dict[Key, Entry]:
    """Read plain shards only; an absent directory means untranslated Q&A."""
    root = authored / DIRECTORY
    check_path(authored, root)
    if not root.exists():
        return {}
    if root.is_symlink() or not root.is_dir():
        raise ValueError("Q&A translation input must be a plain directory")
    result: dict[Key, Entry] = {}
    for path in sorted(root.iterdir()):
        if path.suffix != ".yaml" or path.is_symlink() or not path.is_file():
            raise ValueError("Q&A translation directory holds only YAML files")
        try:
            shard = Shard.model_validate_json(canonical(read_yaml(path)))
        except ValidationError:
            raise ValueError(f"Invalid Q&A translation shard {path.name}") from None
        for entry in shard.entries:
            key = entry.key()
            if key in result:
                raise ValueError("Q&A source field appears in two shards")
            result[key] = entry
    return result


def sources(db: Database) -> dict[Key, str]:
    """Collect Japanese fields with their Q&A identity and exact source hash."""
    japanese = {r.values["id"] for r in db.rows("qa") if r.values["region"] == "jp"}
    units = {str(r.values["id"]): r.values for r in db.rows("text_unit")}
    return {
        (
            str(version.values["qa_id"]),
            field,
            str(unit["content_hash"]),
            "zh-Hant",
        ): str(unit["text"])
        for version in db.rows("qa_version")
        if version.values["qa_id"] in japanese
        for field in FIELDS
        if (unit := units[str(version.values[field + "_unit_id"])])["lang"] == "ja"
    }


def apply(db: Database, entries: dict[Key, Entry]) -> Report:
    """A changed field cannot inherit a translation from an older source hash."""
    japanese = {r.values["id"] for r in db.rows("qa") if r.values["region"] == "jp"}
    units = {r.values["id"]: r.values for r in db.rows("text_unit")}
    used: set[Key] = set()
    applied = 0
    for version in db.rows("qa_version"):
        if version.values["qa_id"] not in japanese:
            continue
        for field in FIELDS:
            unit = units[version.values[field + "_unit_id"]]
            key = (
                str(version.values["qa_id"]),
                field,
                str(unit["content_hash"]),
                "zh-Hant",
            )
            entry = entries.get(key)
            if unit["lang"] != "ja" or entry is None:
                continue
            write(
                db,
                {"qa_version_id": str(version.values["id"])},
                field=field,
                lang=entry.lang,
                source_unit_id=str(unit["id"]),
                text=entry.text,
                origin=entry.origin,
                low_confidence=entry.low_confidence,
                semantic_variant=str(version.values["qa_id"]) + ":" + field,
            )
            used.add(key)
            applied += 1
    return Report(len(entries), applied, len(entries) - len(used))
