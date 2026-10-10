"""Replay explicitly applied effect corrections without changing sealed source bytes."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.domains.source_corrections.plan import verify_applications

if TYPE_CHECKING:
    from collections.abc import Mapping

    from pydantic import JsonValue

    from sve_carddb.core.provenance import BuildContext
    from sve_carddb.domains.text_observations.plan import TextPlan

PARSER = "translation-jp-corrected-v1"


@dataclass(frozen=True)
class CorrectedField:
    card_id: str
    face_id: str
    original: str
    corrected: str


@dataclass(frozen=True, init=False)
class Corrections:
    pin: str
    fields: Mapping[tuple[str, str, int], CorrectedField]

    def __init__(self, plan: TextPlan) -> None:
        if plan.corrections is None:
            raise ValueError(
                "Corrected translation sources require a pinned correction plan"
            )
        verify_applications(plan.identity, plan.observations, plan.corrections)
        fields: dict[tuple[str, str, int], CorrectedField] = {}
        for application in plan.corrections:
            item = application.observation
            if (
                item.region != "jp"
                or application.status != "applied"
                or application.data.field != "effect"
            ):
                continue
            source = item.card.source
            key = source.archive.batch_id, source.id, item.source_index
            field = CorrectedField(
                item.card_id,
                item.face_id,
                application.data.expected_raw_value,
                application.data.corrected_value,
            )
            if key in fields and fields[key] != field:
                raise ValueError("Ambiguous corrected translation source field")
            fields[key] = field
        object.__setattr__(
            self, "pin", string(plan.configuration()["corrections_hash"])
        )
        object.__setattr__(self, "fields", MappingProxyType(fields))

    def verify_context(self, build: BuildContext) -> None:
        """Reject reuse under a build that did not select these correction facts."""
        configuration = object_value(parse(build.configuration.encode()))
        plan = object_value(configuration.get("text_observations"))
        if plan.get("corrections_hash") != self.pin:
            raise ValueError(
                "Translation source context does not pin its correction plan"
            )

    def project(self, batch: str, version: str, document: JsonValue) -> JsonValue:
        """Preserve the direct projection while applying only exact verified fields."""
        fields = [
            (index, value)
            for (bid, sid, index), value in self.fields.items()
            if (bid, sid) == (batch, version)
        ]
        if not fields:
            raise ValueError(
                "Corrected translation recipe has no applied effect correction"
            )
        result = parse(canonical(document))
        faces = array(object_value(result).get("faces"))
        for index, field in fields:
            if index >= len(faces):
                raise ValueError("Corrected source face is absent from the frozen page")
            face = object_value(faces[index])
            if face.get("text") != field.original:
                raise ValueError("Corrected source differs from exact original effect")
            face["text"] = field.corrected
        return result

    def verify_scope(
        self, batch: str, version: str, index: int, card_id: str, face_id: str
    ) -> None:
        """An equal corrected value on another card does not authorize its owner."""
        field = self.fields.get((batch, version, index))
        if field is None or (field.card_id, field.face_id) != (card_id, face_id):
            raise ValueError("Corrected translation source belongs to another owner")
