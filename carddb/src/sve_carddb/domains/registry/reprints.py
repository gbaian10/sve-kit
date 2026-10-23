"""Load confirmed reprint printings independently of card number and rarity."""

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

FILE = "catalog/reprints.yaml"
ReprintBasis = Literal["series_numbering"]


class ReprintRecord(RecordData):
    printing_id: str
    region: Region
    card_no: str
    basis: ReprintBasis


class Reprints(RecordData):
    format: Literal[1]
    kind: Literal["printing_reprints"]
    records: tuple[ReprintRecord, ...]

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Reprint format must be an integer")
        return value


def load_reprints(authored: Path) -> tuple[ReprintRecord, ...]:
    """Read the single working-tree file through the strict authored YAML boundary."""
    path = authored / FILE
    check_path(authored, path)
    data = path.read_bytes()
    if len(data) >= MAX_BYTES:
        raise ValueError("Oversized reprint YAML")
    try:
        raw = JSON_VALUE.validate_python(parse_yaml(data), strict=True)
        records = Reprints.model_validate_json(canonical(raw)).records
    except ValidationError as error:
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise ValueError("Invalid reprint fields: " + details) from None
    if len({record.printing_id for record in records}) != len(records):
        raise ValueError("Duplicate reprint printing_id")
    return records


def populate_reprints(db: Database, records: tuple[ReprintRecord, ...]) -> None:
    """Only registered printings are reprints; the card number never decides."""
    printings = {str(row.values["id"]): row.values for row in db.rows("printing")}
    for record in records:
        printing = printings.get(record.printing_id)
        if printing is None:
            raise ValueError("Unknown reprint printing_id: " + record.printing_id)
        if (printing["region"], printing["card_no"]) != (record.region, record.card_no):
            raise ValueError("Reprint identity mismatch: " + record.printing_id)
        db.update("printing", {"id": record.printing_id}, {"reprint": True})
