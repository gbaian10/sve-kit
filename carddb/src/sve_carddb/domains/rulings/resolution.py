"""Preserve unknown legacy references without granting executable applicability."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.rulings import (
    Legacy,
    Resolution,
    RetainedReference,
    RulingRef,
)
from sve_carddb.core.json import canonical

if TYPE_CHECKING:
    from sve_carddb.domains.rulings.reader import Document


@dataclass(frozen=True)
class Report:
    resolutions: tuple[Resolution, ...]
    retained: tuple[RetainedReference, ...]

    def __post_init__(self) -> None:
        """Physical row order cannot replace the original reference ordinal."""
        object.__setattr__(
            self,
            "resolutions",
            tuple(
                sorted(
                    self.resolutions,
                    key=lambda r: (
                        r.ruling_ref.id,
                        r.ruling_ref.revision,
                        r.ruling_ref.reference_ordinal,
                        b""
                        if r.legacy.occurrence is None
                        else canonical(r.legacy.occurrence.model_dump(mode="json")),
                    ),
                )
            ),
        )
        object.__setattr__(
            self,
            "retained",
            tuple(
                sorted(
                    self.retained,
                    key=lambda r: (
                        r.ruling_ref.id,
                        r.ruling_ref.revision,
                        r.ruling_ref.reference_ordinal,
                    ),
                )
            ),
        )

    def payload(self) -> dict[str, JsonValue]:
        """Readable pending records are kept separate from active applicability counts."""
        return {
            "references": len(self.retained)
            + len(
                {
                    canonical(r.ruling_ref.model_dump(mode="json"))
                    for r in self.resolutions
                }
            ),
            "template_references": len(
                {
                    canonical(r.ruling_ref.model_dump(mode="json"))
                    for r in self.resolutions
                }
            ),
            "retained_non_template": len(self.retained),
            "reference_pending": sum(r.level == "reference" for r in self.resolutions),
            "occurrence_resolved": sum(
                r.level == "occurrence" and r.status == "resolved"
                for r in self.resolutions
            ),
            "occurrence_pending": sum(
                r.level == "occurrence" and r.status == "pending"
                for r in self.resolutions
            ),
            "active_template_edges": sum(
                r.status == "resolved" for r in self.resolutions
            ),
            "resolutions": [r.model_dump(mode="json") for r in self.resolutions],
            "retained": [r.model_dump(mode="json") for r in self.retained],
        }


def build(documents: tuple[Document, ...]) -> Report:
    """Original list positions distinguish repeated historical references."""
    resolutions = []
    retained = []
    for document in documents:
        ruling = document.ruling
        for ordinal, identifier in enumerate(ruling.applies_to):
            ref = RulingRef(
                id=ruling.id, revision=ruling.revision, reference_ordinal=ordinal
            )
            if re.fullmatch(r"[EDL]\.[a-z][a-z0-9_.-]*", identifier):
                retained.append(
                    RetainedReference(
                        ruling_ref=ref,
                        ruling_source=document.source,
                        reference_kind="ir_element",
                        reference_id=identifier,
                        disposition="retained_non_template",
                    )
                )
            elif re.fullmatch(r"T[0-9a-f]{10}", identifier):
                resolutions.append(
                    Resolution(
                        ruling_ref=ref,
                        ruling_source=document.source,
                        level="reference",
                        legacy=Legacy(
                            namespace=None, template_id=identifier, occurrence=None
                        ),
                        status="pending",
                        reason="unknown_legacy_scope",
                        target=None,
                        candidates=(),
                    )
                )
            else:
                raise ValueError("Unsupported ruling applicability reference")
    return Report(tuple(resolutions), tuple(retained))
