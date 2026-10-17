"""Product-family display labels use the shared current translation shard."""

from typing import TYPE_CHECKING, Literal

from pydantic import computed_field, field_validator

from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.domains.products.models import Lang
from sve_carddb.domains.translations.direct import write
from sve_carddb.domains.translations.models import Quality

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.domains.translations.four_layer_authored import Inputs


class FamilyLabel(RecordData):
    public_code: Text
    lang: Lang
    text: Text

    @field_validator("public_code", "text")
    @classmethod
    def _nonblank(cls, value: str) -> str:
        if not value.strip() or value != value.strip():
            raise ValueError(
                "Family label code and text must be nonblank without outer space"
            )
        return value


class FamilyLabelRecord(Quality):
    kind: Literal["product_family_label"]
    data: FamilyLabel

    @field_validator("origin")
    @classmethod
    def _authored(cls, value: str) -> str:
        if value == "official":
            raise ValueError(
                "Family label only supports project or machine translations"
            )
        return value

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes the property; mypy cannot compose decorators.
    @property
    def record_key(self) -> str:
        """The public family code and language select one editable current label."""
        return canonical([self.kind, self.data.public_code, self.data.lang]).decode()


def apply(db: Database, inputs: Inputs) -> None:
    """Validate all referenced families before inserting their label translations."""
    families = {
        row.values["public_code"]: row.values for row in db.rows("product_family")
    }
    records = [r for r in inputs.records if isinstance(r, FamilyLabelRecord)]
    for record in records:
        if record.data.public_code not in families:
            raise ValueError(
                f"Unknown family label public_code: {record.data.public_code}"
            )
    for record in records:
        family = families[record.data.public_code]
        if record.origin == "official":
            raise ValueError("Family label cannot claim official authority")
        write(
            db,
            {"product_family_id": str(family["id"])},
            field="label",
            lang=record.data.lang,
            source_unit_id=str(family["name_unit_id"]),
            text=record.data.text,
            origin=record.origin,
            low_confidence=record.low_confidence,
            semantic_variant="product_family:" + str(family["id"]),
        )
