"""Load confirmed leader name frames independently of rarity."""

from typing import TYPE_CHECKING, Literal

from pydantic import ValidationError, field_validator

from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData
from sve_carddb.core.regions import Region
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, parse_yaml

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database

FILE = "catalog/leader-name-frames.yaml"
LeaderNameFrame = Literal["gold", "white"]


class LeaderFrameRecord(RecordData):
    printing_id: str
    region: Region
    card_no: str
    leader_name_frame: LeaderNameFrame


class LeaderFrames(RecordData):
    format: Literal[1]
    kind: Literal["printing_leader_name_frames"]
    records: tuple[LeaderFrameRecord, ...]

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Leader frame format must be an integer")
        return value


def load_leader_frames(authored: Path) -> tuple[LeaderFrameRecord, ...]:
    """Read the single working-tree file through the strict authored YAML boundary."""
    path = authored / FILE
    check_path(authored, path)
    data = path.read_bytes()
    if len(data) >= MAX_BYTES:
        raise ValueError("Oversized leader frame YAML")
    try:
        raw = JSON_VALUE.validate_python(parse_yaml(data), strict=True)
        records = LeaderFrames.model_validate_json(canonical(raw)).records
    except ValidationError as error:
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise ValueError("Invalid leader frame fields: " + details) from None
    if len({record.printing_id for record in records}) != len(records):
        raise ValueError("Duplicate leader frame printing_id")
    return records


def populate_leader_frames(
    db: Database, records: tuple[LeaderFrameRecord, ...]
) -> None:
    """Only confirmed leader printings may receive a name frame."""
    printings = {str(row.values["id"]): row.values for row in db.rows("printing")}
    types: dict[tuple[str, str], set[str]] = {}
    for row in db.rows("face_revision"):
        values = row.values
        key = (str(values["face_id"]), str(values["region"]))
        types.setdefault(key, set()).add(str(values["type_code"]))
    faces: dict[str, set[str]] = {}
    for row in db.rows("printing_face"):
        values = row.values
        faces.setdefault(str(values["printing_id"]), set()).add(str(values["face_id"]))
    for record in records:
        printing = printings.get(record.printing_id)
        if printing is None:
            raise ValueError("Unknown leader frame printing_id: " + record.printing_id)
        if (printing["region"], printing["card_no"]) != (record.region, record.card_no):
            raise ValueError("Leader frame identity mismatch: " + record.printing_id)
        printing_types = set().union(
            *(
                types.get((face, record.region), set())
                for face in faces.get(record.printing_id, set())
            )
        )
        if printing_types != {"leader"}:
            raise ValueError(
                "Leader frame requires a leader printing: " + record.printing_id
            )
        db.update(
            "printing",
            {"id": record.printing_id},
            {"leader_name_frame": record.leader_name_frame},
        )
