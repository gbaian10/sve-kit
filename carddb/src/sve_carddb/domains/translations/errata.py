"""Source-paired historical fragments cannot borrow a current card's owner binding."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, ValidationError

from sve_carddb.build import Json
from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Date, Hash, RecordData, Text
from sve_carddb.domains.registry.storage import read_yaml

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database


class Entry(RecordData):
    source_hash: Hash
    source_url: Text
    source_date: Date | None
    lang: Literal["zh-Hant"]
    before: Text
    after: Text
    origin: Literal["machine", "project"]
    low_confidence: bool


class Shard(RecordData):
    format: Literal[1]
    kind: Literal["errata_translations"]
    entries: tuple[Entry, ...]


@dataclass(frozen=True)
class Inputs:
    files: tuple[tuple[str, bytes, Shard], ...]

    def configuration(self) -> dict[str, JsonValue]:
        """Pin uncommitted authored edits in the build configuration too."""
        return {name: digest(raw) for name, raw, _ in self.files}


def load(authored: Path) -> Inputs:
    """Validate this build's current shard inputs once, as for flavor translations."""
    directory = authored / "translations" / "errata"
    check_path(authored, directory)
    files = []
    if not directory.exists():
        return Inputs(())
    for path in sorted(directory.iterdir()):
        if path.suffix != ".yaml" or path.is_symlink() or not path.is_file():
            raise ValueError("Errata translations require plain YAML shards")
        try:
            value = Shard.model_validate_json(canonical(read_yaml(path)))
        except ValidationError:
            raise ValueError("Invalid errata translation shard") from None
        files.append((path.name, path.read_bytes(), value))
    return Inputs(tuple(files))


def _stage(
    db: Database, inputs: Inputs, revision: str
) -> dict[tuple[str, str, str], tuple[Entry, str]]:
    entries: dict[tuple[str, str, str], tuple[Entry, str]] = {}
    for name, raw, shard in inputs.files:
        source_id = (
            "authored:errata:" + digest(canonical([revision, name, digest(raw)]))[7:]
        )
        db.insert(
            "source_record",
            {
                "id": source_id,
                "kind": "authored",
                "sha256": digest(raw),
                "authored_path": "authored/translations/errata/" + name,
                "authored_revision": revision,
                "parser_version": "errata-translation-v1",
            },
        )
        for entry in shard.entries:
            key = (entry.source_url, entry.source_hash, entry.lang)
            if key in entries:
                raise ValueError("Duplicate errata translation")
            entries[key] = (entry, source_id)
    return entries


def apply(db: Database, inputs: Inputs, *, revision: str) -> dict[str, JsonValue]:
    """Match both exact Japanese fragments and their announcement URL before writing."""
    entries = _stage(db, inputs, revision)
    notices = {row.values["id"]: row.values for row in db.rows("errata")}
    versions = {row.values["id"]: row.values for row in db.rows("errata_version")}
    applied = 0
    used: set[tuple[str, str, str]] = set()
    for row in db.rows("errata_change"):
        change = row.values
        version = versions[change["errata_version_id"]]
        notice = notices[version["errata_id"]]
        if notice["region"] != "jp":
            continue
        before, after = change["before_value"], change["after_value"]
        assert isinstance(before, Json)
        assert isinstance(after, Json)
        source_hash = digest(canonical([before.value, after.value]))
        key = (str(notice["official_url"]), source_hash, "zh-Hant")
        found = entries.get(key)
        if found is None:
            continue
        entry, source_id = found
        if entry.source_date != version["announced_on"]:
            raise ValueError("Errata translation announcement date differs")
        db.insert(
            "errata_change_translation",
            {
                "change_id": change["id"],
                "lang": entry.lang,
                "before_text": entry.before,
                "after_text": entry.after,
                "origin": entry.origin,
                "low_confidence": entry.low_confidence,
                "source_id": source_id,
            },
        )
        applied += 1
        used.add(key)
    return {
        "entries": len(entries),
        "applied": applied,
        "unused": len(entries) - len(used),
    }
