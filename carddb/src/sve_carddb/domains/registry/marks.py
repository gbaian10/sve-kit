"""Reviewed printing marks are build inputs; image detection is an author tool."""

from typing import TYPE_CHECKING, Literal

from pydantic import ValidationError, field_validator

from sve_carddb.build import Json
from sve_carddb.core.authored import shards
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.regions import Region

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database

AREA = "catalog/printing-marks"


class Stamp(RecordData):
    kind: Text | None
    year: Text | None


class PrintingMark(RecordData):
    printing_id: Text
    region: Region
    card_no: Text
    stamp: Stamp | None
    serial: bool


class MarkShard(RecordData):
    format: Literal[1]
    kind: Literal["printing_marks_shard"]
    records: tuple[PrintingMark, ...]

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Printing marks format must be an integer")
        return value


def load_marks(authored: Path) -> tuple[PrintingMark, ...]:
    """Read bounded shards, keeping unreviewed printings distinct from reviewed false."""
    result: list[PrintingMark] = []
    for _, _, content in shards(authored, (AREA,), optional=(AREA,)):
        try:
            result.extend(MarkShard.model_validate_json(content).records)
        except ValidationError as error:
            details = "; ".join(
                ".".join(map(str, item["loc"])) + ":" + item["type"]
                for item in error.errors(include_input=False, include_context=False)
            )
            raise ValueError("Invalid printing marks: " + details) from None
    if len({record.printing_id for record in result}) != len(result):
        raise ValueError("Duplicate printing mark printing_id")
    return tuple(result)


def populate_marks(db: Database, records: tuple[PrintingMark, ...]) -> None:
    """Apply only explicitly reviewed marks, independently of rarity or card number."""
    printings = {str(row.values["id"]): row.values for row in db.rows("printing")}
    for record in records:
        printing = printings.get(record.printing_id)
        if printing is None:
            raise ValueError("Unknown printing mark printing_id: " + record.printing_id)
        if (printing["region"], printing["card_no"]) != (record.region, record.card_no):
            raise ValueError("Printing mark identity mismatch: " + record.printing_id)
        db.update(
            "printing",
            {"id": record.printing_id},
            {
                "stamp": None
                if record.stamp is None
                else Json(record.stamp.model_dump(mode="json")),
                "serial": record.serial,
            },
        )
