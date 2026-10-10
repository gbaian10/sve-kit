"""Whole-field English display exceptions share targets without defining rule frames."""

from typing import Annotated, Literal, Self

from pydantic import Field, computed_field, model_validator

from sve_carddb.contracts.four_layer import Code, Hash, LeafRef, LiteralNode, Reference
from sve_carddb.contracts.source_binding import SourceDescriptor
from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.domains.translations.models import Quality


class EnglishTarget(RecordData):
    id: Code
    source_hash: Hash
    lang: Literal["zh-Hant"]
    references: dict[Code, Reference]
    nodes: Annotated[
        tuple[Annotated[LiteralNode | LeafRef, Field(discriminator="kind")], ...],
        Field(min_length=1),
    ]

    @model_validator(mode="after")
    def _required_references(self) -> Self:
        used = {node.slot for node in self.nodes if isinstance(node, LeafRef)}
        if used != self.references.keys():
            raise ValueError("English target must use every required reference")
        return self


class EnglishTargetRecord(Quality):
    kind: Literal["english_exception_target"]
    origin: Literal["project", "machine"] = "project"
    data: EnglishTarget

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes the property.
    @property
    def record_key(self) -> str:
        """Shared targets are independent of the printing that selects them."""
        return canonical([self.kind, self.data.id, self.data.lang]).decode()


class EnglishUse(RecordData):
    source: SourceDescriptor
    card_id: Text
    face_id: Text
    target_id: Code
    identity: Literal["confirmed_no_jp", "unresolved"]
    reason: Text

    @model_validator(mode="after")
    def _purpose(self) -> Self:
        if self.source.owner.kind not in {"face_revision", "printing_face"}:
            raise ValueError("English exception requires a card owner")
        if self.source.field not in {"name", "effect", "section", "flavor"}:
            raise ValueError("English exception requires a whole display field")
        if not self.reason.strip():
            raise ValueError("English identity conclusion requires a reason")
        if self.source.source_ref.parser != "translation-en-v1":
            raise ValueError("English exception requires an English source recipe")
        return self


class EnglishUseRecord(Quality):
    kind: Literal["english_exception_use"]
    origin: Literal["project", "machine"] = "project"
    data: EnglishUse

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes the property.
    @property
    def record_key(self) -> str:
        """An owner field has one explicit display selection."""
        source = self.data.source
        return canonical(
            [
                self.kind,
                source.owner.model_dump(mode="json"),
                source.field,
                source.ordinal,
            ]
        ).decode()
