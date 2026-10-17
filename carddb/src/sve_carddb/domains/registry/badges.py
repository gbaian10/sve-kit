"""Load printing badge appearances independently of official rarity."""

from typing import TYPE_CHECKING, Literal

from pydantic import ValidationError, field_validator, model_validator

from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData
from sve_carddb.core.regions import Region
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, parse_yaml

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build import Database

FILE = "catalog/rarity-badges.yaml"
BadgeStyle = Literal["bronze", "silver", "gold", "red", "pastel", "rainbow", "none"]


class BadgeRecord(RecordData):
    printing_id: str
    region: Region
    card_no: str
    style: BadgeStyle
    text: str | None = None

    @model_validator(mode="after")
    def _text(self) -> BadgeRecord:
        if self.style == "none":
            if "text" in self.model_fields_set:
                raise ValueError("A missing badge must omit text")
        elif self.text is None or not self.text.strip():
            raise ValueError("A visible badge requires its printed text")
        return self


class BadgeOverrides(RecordData):
    format: Literal[1]
    kind: Literal["printing_rarity_badges"]
    records: tuple[BadgeRecord, ...]

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Badge format must be an integer")
        return value


def load_badges(authored: Path) -> tuple[BadgeRecord, ...]:
    """Read the single working-tree file through the strict authored YAML boundary."""
    path = authored / FILE
    check_path(authored, path)
    data = path.read_bytes()
    if len(data) >= MAX_BYTES:
        raise ValueError("Oversized badge YAML")
    try:
        raw = JSON_VALUE.validate_python(parse_yaml(data), strict=True)
        records = BadgeOverrides.model_validate_json(canonical(raw)).records
    except ValidationError as error:
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise ValueError("Invalid badge fields: " + details) from None
    if len({record.printing_id for record in records}) != len(records):
        raise ValueError("Duplicate badge printing_id")
    return records


def populate_badges(
    db: Database, records: tuple[BadgeRecord, ...]
) -> list[dict[str, JsonValue]]:
    """Apply appearances and report official PR printings still awaiting annotation."""
    printings = {str(row.values["id"]): row.values for row in db.rows("printing")}
    for record in records:
        if record.printing_id not in printings:
            raise ValueError("Unknown badge printing_id: " + record.printing_id)
        db.update(
            "printing",
            {"id": record.printing_id},
            {"rarity_badge_style": record.style, "rarity_badge_text": record.text},
        )
    annotated = {record.printing_id for record in records}
    return [
        {
            "printing_id": key,
            "region": str(row["region"]),
            "card_no": str(row["card_no"]),
        }
        for key, row in sorted(printings.items())
        if row["rarity_code"] == "pr" and key not in annotated
    ]
