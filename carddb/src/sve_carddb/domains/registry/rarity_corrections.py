"""Correct an official rarity listing that the printed card contradicts."""

from typing import TYPE_CHECKING, Annotated, Literal

from pydantic import Field, ValidationError, field_validator, model_validator

from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.regions import Region
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, parse_yaml
from sve_carddb.domains.registry.rarity import normalize
from sve_carddb.domains.registry.records import CorrectionEvidence

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build import Database

FILE = "catalog/rarity-corrections.yaml"


class RarityCorrection(RecordData):
    printing_id: str
    region: Region
    card_no: str
    expected_raw_value: Text
    corrected_value: Text
    reason: Text
    evidence: Annotated[tuple[CorrectionEvidence, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _values(self) -> RarityCorrection:
        if self.expected_raw_value == self.corrected_value:
            raise ValueError("A rarity correction must change the value")
        # Either label must be one the normalizer knows, so a correction never adopts a
        # spelling that would only resurface as an unrecognized rarity.
        for value in (self.expected_raw_value, self.corrected_value):
            if normalize(self.region, value) is None:
                raise ValueError("Unrecognized rarity label: " + value)
        if any(item.region != self.region for item in self.evidence):
            raise ValueError("Rarity correction evidence must match its region")
        return self


class RarityCorrections(RecordData):
    format: Literal[1]
    kind: Literal["printing_rarity_corrections"]
    records: tuple[RarityCorrection, ...]

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Rarity correction format must be an integer")
        return value


def load_rarity_corrections(authored: Path) -> tuple[RarityCorrection, ...]:
    """Read the single working-tree file through the strict authored YAML boundary."""
    path = authored / FILE
    check_path(authored, path)
    data = path.read_bytes()
    if len(data) >= MAX_BYTES:
        raise ValueError("Oversized rarity correction YAML")
    try:
        raw = JSON_VALUE.validate_python(parse_yaml(data), strict=True)
        records = RarityCorrections.model_validate_json(canonical(raw)).records
    except ValidationError as error:
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise ValueError("Invalid rarity correction fields: " + details) from None
    if len({record.printing_id for record in records}) != len(records):
        raise ValueError("Duplicate rarity correction printing_id")
    return records


def populate_rarity_corrections(
    db: Database, records: tuple[RarityCorrection, ...]
) -> list[dict[str, JsonValue]]:
    """Replace staged raw labels before normalization; report upstream fixes."""
    printings = {str(row.values["id"]): row.values for row in db.rows("printing")}
    diagnostics: list[dict[str, JsonValue]] = []
    for record in records:
        printing = printings.get(record.printing_id)
        if printing is None:
            raise ValueError(
                "Unknown rarity correction printing_id: " + record.printing_id
            )
        if (printing["region"], printing["card_no"]) != (record.region, record.card_no):
            raise ValueError(
                "Rarity correction identity mismatch: " + record.printing_id
            )
        raw = printing["rarity_raw"]
        if raw == record.corrected_value:
            diagnostics.append(
                {
                    "printing_id": record.printing_id,
                    "region": record.region,
                    "rarity_raw": raw,
                    "reason": "correction_upstream_fixed",
                }
            )
        elif raw == record.expected_raw_value:
            db.update(
                "printing",
                {"id": record.printing_id},
                {"rarity_raw": record.corrected_value},
            )
        else:
            raise ValueError("Rarity correction conflict: " + record.printing_id)
    return diagnostics
