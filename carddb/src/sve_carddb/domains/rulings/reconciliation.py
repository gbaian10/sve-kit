"""Keep candidate diagnostics separate from verified ruling resolution and IR uses."""

from collections import Counter
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.rulings import LegacyReference
from sve_carddb.domains.effects.matching import Matcher
from sve_carddb.domains.rulings.resolution import build

if TYPE_CHECKING:
    from sve_carddb.domains.effects.classifications import Member
    from sve_carddb.domains.effects.matching import FrameOccurrence, Match
    from sve_carddb.domains.effects.reader import Catalog
    from sve_carddb.domains.rulings.mapping import Rebuild
    from sve_carddb.domains.rulings.reader import Document


def _member(member: Member, match: Match) -> dict[str, JsonValue]:
    return {
        "member_id": member.id,
        "position": member.position.model_dump(mode="json"),
        "status": match.status,
        "candidate_bindings": list[JsonValue](
            sorted(c.binding_id for c in match.candidates)
        ),
        "exact_bindings": list[JsonValue](sorted(c.binding_id for c in match.exact)),
        "selected_bindings": list[JsonValue](
            sorted(c.binding_id for c in match.selected)
        ),
        "selected_frames": list[JsonValue](
            sorted({c.frame_id for c in match.selected})
        ),
    }


def _category(matches: tuple[Match, ...]) -> str:
    if not matches:
        return "missing_definition"
    if any(m.status in {"source_changed", "no_candidate"} for m in matches):
        return "unmatched_member"
    if any(any(not c.current for c in m.selected) for m in matches):
        return "historical_source"
    if (
        any(m.status == "ambiguous" for m in matches)
        or len({c.frame_id for m in matches for c in m.selected}) > 1
    ):
        return "multiple_frames"
    return "single_current_frame"


def reconcile(
    documents: tuple[Document, ...],
    catalog: Catalog,
    frames: tuple[FrameOccurrence, ...],
    *,
    rebuild: Rebuild | None = None,
) -> dict[str, JsonValue]:
    """A unique body match is still only a diagnostic without a complete old domain."""
    report = build(documents, rebuild)
    members = {m.id: m for m in catalog.members}
    classifications = {c.id: c for c in catalog.classifications}
    matcher = Matcher(frames)
    matches: dict[str, Match] = {}
    references: list[JsonValue] = []
    categories: Counter[str] = Counter()
    template_count = 0
    retained = {
        (r.ruling_ref.id, r.ruling_ref.revision, r.ruling_ref.reference_ordinal)
        for r in report.retained
    }
    for document in documents:
        ruling = document.ruling
        for ordinal, original in enumerate(ruling.applies_to):
            reference: dict[str, JsonValue] = {
                "ruling_ref": {
                    "id": ruling.id,
                    "revision": ruling.revision,
                    "reference_ordinal": ordinal,
                },
                "ruling_source": document.source.model_dump(mode="json"),
                "original": original.legacy
                if isinstance(original, LegacyReference)
                else original,
            }
            if (ruling.id, ruling.revision, ordinal) in retained:
                reference.update(kind="ir", category="retained_non_template")
            else:
                template_count += 1
                classification = classifications.get(
                    original.legacy
                    if isinstance(original, LegacyReference)
                    else original
                )
                selected = (
                    ()
                    if classification is None
                    else tuple(members[i] for i in classification.members)
                )
                for member in selected:
                    if member.id not in matches:
                        matches[member.id] = matcher.match(member)
                category = _category(tuple(matches[m.id] for m in selected))
                categories[category] += 1
                reference.update(
                    kind="template",
                    category=category,
                    members=[_member(m, matches[m.id]) for m in selected],
                )
            references.append(reference)
    return {
        "classification_namespace": catalog.namespace,
        "classification_basis": catalog.basis.model_dump(mode="json"),
        "reference_count": len(references),
        "template_reference_count": template_count,
        "ir_reference_count": len(retained),
        "candidate_categories": dict(sorted(categories.items())),
        "references": references,
        "resolution": report.payload(),
    }
