"""Product-family display labels use the shared current translation shard."""

import re
from typing import TYPE_CHECKING, Literal, Self

from pydantic import computed_field, field_validator, model_validator

from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.domains.catalog.adoption_models import SourceRef  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves this inherited model field at runtime.
from sve_carddb.domains.products.models import Lang
from sve_carddb.domains.translations.direct import write
from sve_carddb.domains.translations.models import Quality

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.domains.translations.four_layer_authored import Inputs
    from sve_carddb.domains.translations.sources import Sources


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
    source_ref: SourceRef | None = None

    @model_validator(mode="after")
    def _source(self) -> Self:
        if self.origin == "official":
            if (
                self.source_ref is None
                or self.source_ref.parser != "translation-en-v1"
                or re.fullmatch(
                    r"/products/(0|[1-9][0-9]*)/name", self.source_ref.locator
                )
                is None
                or self.data.lang != "en"
            ):
                raise ValueError(
                    "Official family label requires an EN product name source"
                )
        elif self.source_ref is not None:
            raise ValueError("Unofficial family label cannot claim an official source")
        return self

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes the property; mypy cannot compose decorators.
    @property
    def record_key(self) -> str:
        """The public family code and language select one editable current label."""
        return canonical([self.kind, self.data.public_code, self.data.lang]).decode()


def apply(db: Database, inputs: Inputs, *, sources: Sources | None = None) -> None:
    """Validate all referenced families before inserting their label translations."""
    families = {
        row.values["public_code"]: row.values for row in db.rows("product_family")
    }
    records = [r for r in inputs.records if isinstance(r, FamilyLabelRecord)]
    source_ids: dict[str, str] = {}
    for record in records:
        if record.data.public_code not in families:
            raise ValueError(
                f"Unknown family label public_code: {record.data.public_code}"
            )
        if record.source_ref is not None:
            if sources is None:
                raise ValueError("Official family labels require frozen sources")
            lang, text, source = sources.text(record.source_ref)
            if (lang, text) != (record.data.lang, record.data.text):
                raise ValueError(
                    "Official family label must match exact source text and language"
                )
            source_ids[record.record_key] = source.id
    if source_ids:
        assert sources is not None
        insert_raw_sources(db, (use.source for use in sources.uses))
    for record in records:
        family = families[record.data.public_code]
        write(
            db,
            {"product_family_id": str(family["id"])},
            field="label",
            lang=record.data.lang,
            source_unit_id=str(family["name_unit_id"]),
            text=record.data.text,
            origin=record.origin,
            source_id=source_ids.get(record.record_key),
            low_confidence=record.low_confidence,
            semantic_variant="product_family:" + str(family["id"]),
        )
