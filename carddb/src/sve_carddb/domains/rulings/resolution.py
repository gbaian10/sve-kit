"""Preserve unknown legacy references without granting executable applicability."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.rulings import (
    Legacy,
    LegacyReference,
    Mapping,
    PendingReason,
    Resolution,
    RetainedReference,
    RulingRef,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.rulings.mapping import source_scope

if TYPE_CHECKING:
    from sve_carddb.domains.rulings.mapping import LegacyUse, Namespace, Rebuild
    from sve_carddb.domains.rulings.reader import Document


@dataclass(frozen=True)
class Report:
    resolutions: tuple[Resolution, ...]
    retained: tuple[RetainedReference, ...]
    mappings: tuple[Mapping, ...] = ()
    families: tuple[dict[str, JsonValue], ...] = ()
    namespaces: tuple[Namespace, ...] = ()

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
            "mappings": [m.model_dump(mode="json") for m in self.mappings],
            "frame_families": list(self.families),
            "legacy_namespaces": [n.model_dump(mode="json") for n in self.namespaces],
        }


def _template(
    document: Document, reference: RulingRef, identifier: str, rebuild: Rebuild | None
) -> tuple[Resolution, ...]:
    definitions = () if rebuild is None else rebuild.definitions.get(identifier, ())
    if len(definitions) == 1:
        namespace, definition = definitions[0]
        if definition.uses is not None:
            assert rebuild is not None
            return tuple(
                _occurrence(
                    document, reference, namespace.code, identifier, use, rebuild
                )
                for use in definition.uses
            )
    return (
        Resolution(
            ruling_ref=reference,
            ruling_source=document.source,
            level="reference",
            legacy=Legacy(
                namespace=definitions[0][0].code if len(definitions) == 1 else None,
                template_id=identifier,
                occurrence=None,
            ),
            status="pending",
            reason="unknown_legacy_scope",
            target=None,
            candidates=(),
        ),
    )


def _occurrence(
    document: Document,
    reference: RulingRef,
    namespace: str,
    identifier: str,
    use: LegacyUse,
    rebuild: Rebuild,
) -> Resolution:
    targets = rebuild.candidates(use)
    reason: PendingReason | None
    if len(targets) > 1:
        reason = "ambiguous_variant"
    elif targets and targets[0].semantic_variant.state == "pending":
        reason = "unsupported_relation"
    else:
        reason = None if targets else rebuild.reason(use)
    return Resolution(
        ruling_ref=reference,
        ruling_source=document.source,
        level="occurrence",
        legacy=Legacy(
            namespace=namespace, template_id=identifier, occurrence=use.occurrence
        ),
        status="resolved" if reason is None else "pending",
        target=targets[0] if reason is None else None,
        candidates=targets if reason is not None else (),
        reason=reason,
    )


def build(documents: tuple[Document, ...], rebuild: Rebuild | None = None) -> Report:
    """Each original ordinal expands over its complete old domain or stays reference-pending."""
    resolutions: list[Resolution] = []
    retained = []
    for document in documents:
        ruling = document.ruling
        for ordinal, identifier in enumerate(ruling.applies_to):
            ref = RulingRef(
                id=ruling.id, revision=ruling.revision, reference_ordinal=ordinal
            )
            if isinstance(identifier, LegacyReference):
                if rebuild is None:
                    raise ValueError(
                        "Explicit historical domain requires verified build sources"
                    )
                proven = rebuild.definitions.get(identifier.legacy, ())
                selected = [d for n, d in proven if n.code == identifier.namespace]
                if len(selected) != 1 or selected[0].uses is None:
                    raise ValueError(
                        "Explicit historical domain lacks verified definition"
                    )
                if {(u.occurrence, u.scope) for u in selected[0].uses} != {
                    (
                        u.occurrence,
                        source_scope(u.occurrence),
                    )
                    for u in identifier.uses
                }:
                    raise ValueError(
                        "Explicit historical domain differs from verified definition"
                    )
                resolutions.extend(
                    _occurrence(
                        document,
                        ref,
                        identifier.namespace,
                        identifier.legacy,
                        use,
                        rebuild,
                    )
                    for use in selected[0].uses
                )
            elif re.fullmatch(r"[EDL]\.[a-z][a-z0-9_.-]*", identifier):
                retained.append(
                    RetainedReference(
                        ruling_ref=ref,
                        ruling_source=document.source,
                        reference_kind="ir_element",
                        reference_id=identifier,
                        disposition="retained_non_template",
                    )
                )
            elif re.fullmatch(r"T[0-9a-f]{10}", identifier) or (
                rebuild is not None and identifier in rebuild.definitions
            ):
                resolutions.extend(_template(document, ref, identifier, rebuild))
            else:
                raise ValueError("Unsupported ruling applicability reference")
    return Report(
        tuple(resolutions),
        tuple(retained),
        () if rebuild is None else rebuild.mappings,
        () if rebuild is None else tuple(rebuild.families()),
        () if rebuild is None else rebuild.namespaces,
    )
