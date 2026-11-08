"""Append-only manual-printing shards sharing the registry's YAML and index writer."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.regions import Region
from sve_carddb.domains.manual_printings.records import (
    AREA,
    Key,
    ManualEntry,
    ManualImage,
    ManualName,
    Owner,
    Shard,
    manual_anchor,
)
from sve_carddb.domains.registry.allocation import unlisted_range
from sve_carddb.domains.registry.build import permanent_id
from sve_carddb.domains.registry.storage import Index, encode

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.manual_printings.loader import ManualSnapshot


class ManualSpec(RecordData):
    """One printing to register; IDs and the int_id are allocated by the tool."""

    key: Key
    region: Region
    home_set_id: Owner
    card_type: Literal["ep", "sep"]
    card_no: Text | None
    card_no_state: Literal["none", "official", "provisional"]
    printed_code: Text | None
    name: ManualName
    release_kind: Literal["event", "basic"]
    references: tuple[str, ...]
    image: ManualImage | None


@dataclass(frozen=True)
class Plan:
    files: dict[Path, bytes]
    entries: tuple[ManualEntry, ...]


def plan_manual_files(
    root: Path, existing: ManualSnapshot, index: Index, specs: tuple[ManualSpec, ...]
) -> Plan:
    """Allocate unlisted int_ids from the index cursors and plan new shards only."""
    keys = {data.key for data in existing.printings()}
    if len({spec.key for spec in specs}) != len(specs) or keys & {
        spec.key for spec in specs
    }:
        raise ValueError("Manual printing keys must be new and unique")
    cursor = index.unlisted_cursors()
    entries: list[ManualEntry] = []
    for spec in sorted(specs, key=lambda item: (item.region, item.key)):
        bounds = unlisted_range(spec.region)
        if cursor[spec.region] not in bounds:
            raise ValueError(
                f"Unlisted int_id range exhausted for region {spec.region}"
            )
        anchor = manual_anchor(spec.region, spec.key)
        card_id = permanent_id("c", anchor)
        entries.append(
            # Strict models take tuples only through JSON, as the registry does.
            ManualEntry.model_validate_json(
                canonical(
                    {
                        "kind": "manual_printing",
                        "owner": spec.home_set_id,
                        "data": spec.model_dump(mode="json")
                        | {
                            "id": permanent_id("p", anchor),
                            "card_id": card_id,
                            "face_id": permanent_id("f", card_id + ":0"),
                            "int_id": cursor[spec.region],
                        },
                    }
                )
            )
        )
        cursor[spec.region] += 1
    files: dict[Path, bytes] = {}
    by_owner: dict[str, list[ManualEntry]] = {}
    for entry in entries:
        by_owner.setdefault(entry.owner, []).append(entry)
    for owner, records in sorted(by_owner.items()):
        directory = root / AREA / owner
        number = (
            max((int(path.stem) for path in directory.glob("[0-9]*.yaml")), default=0)
            + 1
        )
        files[directory / f"{number:03}.yaml"] = encode(
            Shard(format=1, kind="manual_printings_shard", records=tuple(records))
        )
    if entries:
        files[root / "ids" / "index.yaml"] = encode(
            Index.model_validate(index.model_dump() | {"next_unlisted_int_id": cursor})
        )
    return Plan(files, tuple(entries))
