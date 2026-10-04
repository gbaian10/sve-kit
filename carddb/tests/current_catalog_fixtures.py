"""Exercise native current catalog composition using synthetic frozen sources."""

from dataclasses import replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.offline import _populate_adoptions, _prepare_catalog
from sve_carddb.snapshot.values import array, canonical, digest, object_value

from .adoption_fixtures import commit, write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import InputRecord
    from sve_carddb.catalog.current import Prepared

    from .adoption_fixtures import Case


def current_case(case: Case) -> Case:
    """Convert only invented fixture inputs, not production adoption records."""
    index_path = case.root / "catalog-adoptions/index.yaml"
    if object_value(read_yaml(index_path))["catalog_adoption_format"] == 2:
        return case
    includes: dict[str, JsonValue] = {}
    for path in (case.root / "catalog-adoptions").rglob("*.yaml"):
        if path.name == "index.yaml":
            continue
        relative = path.relative_to(case.root).as_posix()
        if "/vocabulary/" not in relative and "/languages/" not in relative:
            path.unlink()
            continue
        rows: list[JsonValue] = []
        for raw in array(object_value(read_yaml(path))["records"]):
            record = object_value(raw)
            data = object_value(record["data"])
            rows.append(
                {
                    "record_key": canonical([record["kind"], data["subject"]]).decode(),
                    "kind": record["kind"],
                    "data": {
                        "subject": data["subject"],
                        "value": data["value"],
                        "evidence": record["evidence"],
                    },
                    "origin": "project",
                    "low_confidence": False,
                    "note": "Synthetic current value",
                }
            )
        payload: dict[str, JsonValue] = {
            "catalog_adoption_format": 2,
            "kind": "catalog_adoption_shard",
            "records": sorted(rows, key=lambda r: str(object_value(r)["record_key"])),
        }
        path.write_bytes(canonical(payload))
        includes[relative] = digest(canonical(payload))
    write(
        case.root,
        "catalog-adoptions/index.yaml",
        {
            "catalog_adoption_format": 2,
            "kind": "catalog_adoption_index",
            "includes": dict(sorted(includes.items())),
        },
    )
    return replace(case, revision=commit(case.repository))


def prepare_case(case: Case, stores: dict[str, Path]) -> Prepared:
    case = current_case(case)
    return _prepare_catalog(case.inputs(), case.build(), stores)


def populate_case(db: Database, case: Case, stores: dict[str, Path]) -> InputRecord:
    case = current_case(case)
    with db.transaction():
        return _populate_adoptions(db, case.inputs(), build=case.build(), stores=stores)
