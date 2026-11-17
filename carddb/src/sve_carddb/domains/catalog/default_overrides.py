"""Current-format default printing overrides from catalog/overrides."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build.rows import insert_exact
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.catalog.adoption_loader import load_adoptions
from sve_carddb.domains.catalog.records import DefaultRecord, DisplayShard

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.catalog.adoption_loader import AdoptionSnapshot


def load_overrides(root: Path) -> AdoptionSnapshot | None:
    """The display entry is optional; a checkout without overrides has none to apply."""
    if not (root / "catalog/overrides").exists():
        return None
    snapshot = load_adoptions(root, entry="catalog/overrides")
    if any(not isinstance(r, DefaultRecord) for r in snapshot.current_records()):
        # Route overrides change published URLs and go through identity repair instead.
        raise ValueError("Offline catalog overrides support default printings only")
    return snapshot


@dataclass(frozen=True)
class OverrideReport:
    applied: tuple[str, ...]
    stale: tuple[str, ...]

    def payload(self) -> dict[str, JsonValue]:
        """Record keys only, for the build report."""
        return {
            "applied": list[JsonValue](self.applied),
            "stale": list[JsonValue](self.stale),
        }


def populate_overrides(
    db: Database, snapshot: AdoptionSnapshot | None, revision: str
) -> OverrideReport:
    """Apply a default only while its complete same-card, same-region set is unchanged.

    A changed candidate set needs a new manual choice, so it is reported stale and the
    mechanical selector applies meanwhile; a chosen printing outside its own set is an
    authoring error.
    """
    if snapshot is None:
        return OverrideReport((), ())
    printings: dict[tuple[Value, Value], set[Value]] = {}
    for row in db.rows("printing"):
        key = row.values["card_id"], row.values["region"]
        printings.setdefault(key, set()).add(row.values["id"])
    applied, stale = [], []
    for shard in snapshot.shards:
        source = _source(db, shard.path, shard.exact, revision)
        for record in DisplayShard.model_validate_json(shard.content).records:
            if not isinstance(record, DefaultRecord) or record.data.value is None:
                continue
            value = record.data.value
            if value.printing_id not in value.candidates:
                raise ValueError("Default override selects outside its candidates")
            subject = record.data.subject
            if not db.select("card", ("id",), where={"id": subject.card_id}):
                raise ValueError("Default override names an unknown card")
            current = printings.get((subject.card_id, subject.region), set())
            if current != set(value.candidates):
                stale.append(record.record_key)
                continue
            db.insert(
                "default_printing_override",
                {
                    "card_id": subject.card_id,
                    "region": subject.region,
                    "printing_id": value.printing_id,
                    "decision_id": None,
                    "authored_source_id": source,
                },
            )
            applied.append(record.record_key)
    return OverrideReport(tuple(sorted(applied)), tuple(sorted(stale)))


def _source(db: Database, path: str, exact: bytes, revision: str) -> str:
    identifier = (
        "authored:catalog-current:"
        + digest(canonical([revision, path, digest(exact)]))[7:]
    )
    insert_exact(
        db,
        "source_record",
        {
            "id": identifier,
            "kind": "authored",
            "sha256": digest(exact),
            "authored_path": "authored/" + path,
            "authored_revision": revision,
            "parser_version": "catalog-current-v3",
        },
        ("id",),
    )
    return identifier
