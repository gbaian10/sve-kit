"""Retain all exact observations and select only unambiguous initial current."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build.source_rows import source_values
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.provenance import SourceUse
from sve_carddb.domains.registry.records import CorrectionData, FaceData, PrintingData
from sve_carddb.domains.source_corrections.plan import (
    corrected_observations,
    plan_applications,
    selected_records,
    withheld_regions,
)
from sve_carddb.domains.text_observations.archive import verify_card
from sve_carddb.domains.text_observations.closure import close_preview
from sve_carddb.domains.text_observations.models import FaceObservation
from sve_carddb.domains.text_observations.report import comparisons, observation_report

if TYPE_CHECKING:
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.preview import PreviewPlan
    from sve_carddb.domains.source_corrections.images import ImageProvider
    from sve_carddb.domains.source_corrections.plan import Application
    from sve_carddb.domains.text_observations.models import TextProvider


@dataclass(frozen=True)
class FaceGroup:
    face_id: str
    region: Region
    observations: tuple[FaceObservation, ...]
    reasons: tuple[str, ...]

    def current(self) -> FaceObservation | None:
        """All candidates remain pending when any current-bearing constraint is unknown."""
        return None if self.reasons else self.observations[0]


@dataclass(frozen=True)
class TextPlan:
    identity: PreviewPlan
    observations: tuple[FaceObservation, ...]
    groups: tuple[FaceGroup, ...]
    unavailable: tuple[str, ...]
    diagnostic_exclusions: PreviewPlan
    corrections: tuple[Application, ...] | None = None

    def candidates(self) -> tuple[FaceObservation, ...]:
        """Keep raw source observations immutable while exposing corrected candidates."""
        return corrected_observations(self.observations, self.corrections or ())

    def publication_identity(self) -> PreviewPlan:
        """Unresolved active corrections block output; pending wording is still visible."""
        if self.corrections is None and selected_records(self.identity):
            raise ValueError("Correction output requires pinned image evidence")
        return close_preview(
            self.identity,
            frozenset(
                (application.observation.card_id, application.observation.region)
                for application in self.corrections or ()
                if application.status == "conflict"
            )
            | withheld_regions(self.identity),
        )

    def source_uses(self) -> tuple[SourceUse, ...]:
        """Declare every actual use independently of writes, including null/quarantine."""
        observation_uses = tuple(
            SourceUse(source=item.card.source, usage=usage, locator=item.locator())
            for item in self.observations
            for usage in ("face_text_observation", "face_current_comparison")
        )
        presence_uses = tuple(
            SourceUse(
                source=item.card.source.model_copy(
                    update={"parser_version": proof.result.parser_version}
                ),
                usage="effect_presence",
                locator=canonical(
                    {
                        "printing_id": item.printing_id,
                        "face_id": item.face_id,
                        "source_index": item.source_index,
                        "recipe": proof.result.recipe,
                        "template_id": proof.result.template_id,
                        "container_locator": proof.result.container_locator,
                        "state": proof.result.state,
                        "reason_code": proof.result.reason_code,
                        "result_hash": proof.result_hash,
                    }
                ).decode(),
            )
            for item in self.observations
            if item.card.effect_presence
            for proof in (item.card.effect_presence[item.source_index],)
        )
        return (
            *observation_uses,
            *presence_uses,
            *(
                use
                for application in self.corrections or ()
                for use in application.uses()
            ),
        )

    def configuration(self) -> dict[str, JsonValue]:
        """Pin the observation plan as hashes/identifiers, without any effect text."""
        return {
            "regions": list[JsonValue](self.identity.regions),
            "observations_hash": digest(
                canonical(
                    [
                        {
                            "locator": item.locator(),
                            "source_id": item.card.source.id,
                            "printing_id": item.printing_id,
                            "face_id": item.face_id,
                            "content_hash": item.content.fingerprint(),
                            "raw_face_hash": item.card.faces[
                                item.source_index
                            ].fingerprint(),
                            "effect_presence": item.card.effect_presence[
                                item.source_index
                            ].value()
                            if item.card.effect_presence
                            else None,
                            "has_errata_link": item.card.has_errata_link,
                        }
                        for item in self.observations
                    ]
                )
            ),
            "unavailable_printing_ids": list[JsonValue](self.unavailable),
            "corrections_hash": None
            if self.corrections is None
            else digest(
                canonical(
                    [
                        {
                            "correction_hash": application.key(),
                            "status": application.status,
                            "images": [
                                source.model_dump(mode="json")
                                for source in application.images
                            ],
                        }
                        for application in self.corrections
                    ]
                )
            ),
        }

    def materialized(self) -> tuple[FaceObservation, ...]:
        """Missing effects and excluded identities remain source/report-only observations."""
        included = {
            r.data.id
            for r in self.identity.included("printing")
            if isinstance(r.data, PrintingData)
        }
        return tuple(
            item
            for item in self.observations
            if item.content.effect is not None and item.printing_id in included
        )

    def report(self) -> dict[str, JsonValue]:
        """Separate observations from a diagnostic exclusion proposal awaiting approval."""
        materialized = self.materialized()
        missing = [item for item in self.observations if item.content.effect is None]
        return {
            "total_observations": len(self.observations),
            "materialized_observations": len(materialized),
            "deferred_missing_effect_observations": len(missing),
            "deferred_identity_observations": sum(
                item.content.effect is not None for item in self.observations
            )
            - len(materialized),
            "unavailable_printing_ids": list[JsonValue](self.unavailable),
            "initial_current_count": sum(
                group.current() is not None for group in self.groups
            ),
            "pending_face_region_count": sum(
                bool(group.reasons) for group in self.groups
            ),
            "difference_face_region_count": sum(
                len({item.content.fingerprint() for item in group.observations}) > 1
                for group in self.groups
            ),
            "possible_no_effect_follower_observations": [
                observation_report(item)
                for item in missing
                if item.content.possible_no_effect()
            ],
            "groups": [
                {
                    "face_id": group.face_id,
                    "region": group.region,
                    "card_id": group.observations[0].card_id,
                    "reasons": list[JsonValue](group.reasons),
                    "basis": None if group.reasons else "latest_observed_no_errata",
                    "observations": [
                        observation_report(item) for item in group.observations
                    ],
                    "differences": comparisons(group.observations),
                }
                for group in self.groups
            ],
            "diagnostic_exclusions": self.diagnostic_exclusions.report(),
            "corrections": [
                application.report() for application in self.corrections or ()
            ],
            "publication_identity": None
            if self.corrections is None and selected_records(self.identity)
            else self.publication_identity().report(),
            "diagnostic_exclusion_status": {
                "proposal": "retired-text-exclusion-proposal",
                "publication_gate": False,
                "snapshot_output_authorized": False,
            },
            "remaining_gates": [
                "current_adoption_contract",
                "source_corrections",
                "errata_coverage",
                "public_projection",
                "release_validation",
            ],
        }


def _groups(
    preview: PreviewPlan,
    observations: tuple[FaceObservation, ...],
    unavailable: tuple[str, ...],
    applications: tuple[Application, ...] | None = None,
) -> tuple[FaceGroup, ...]:
    grouped: dict[tuple[str, Region], list[FaceObservation]] = defaultdict(list)
    included = {
        r.data.id
        for r in preview.included("printing")
        if isinstance(r.data, PrintingData)
    }
    corrected: set[tuple[str, Region]] = set()
    for record in preview.snapshot.records.values():
        data = record.data
        if isinstance(data, CorrectionData) and (
            applications is None
            or not any(
                application.data.id == data.id
                and application.status in {"applied", "already_fixed"}
                for application in applications
            )
        ):
            printing = preview.snapshot.records["printing:" + data.printing_id].data
            if isinstance(printing, PrintingData):
                corrected.add((data.face_id, printing.region))
    absent = {
        (mapping.face_id, record.data.region)
        for record in preview.snapshot.records.values()
        if isinstance(record.data, PrintingData) and record.data.id in unavailable
        for mapping in record.data.source_face_map
    }
    for item in observations:
        grouped[item.face_id, item.region].append(item)
    result = []
    for (face_id, region), items in sorted(grouped.items()):
        ordered = tuple(
            sorted(items, key=lambda item: (item.card.source.id, item.printing_id))
        )
        conditions = (
            ("missing_source", (face_id, region) in absent),
            (
                "observation_difference",
                len({item.content.fingerprint() for item in ordered}) != 1,
            ),
            ("missing_effect", any(item.content.effect is None for item in ordered)),
            (
                "identity_pending",
                any(item.printing_id not in included for item in ordered),
            ),
            ("errata_pending", any(item.card.has_errata_link for item in ordered)),
            ("source_correction_pending", (face_id, region) in corrected),
        )
        result.append(
            FaceGroup(
                face_id,
                region,
                ordered,
                tuple(reason for reason, pending in conditions if pending),
            )
        )
    return tuple(result)


def _diagnostic_exclusions(
    preview: PreviewPlan, groups: tuple[FaceGroup, ...]
) -> PreviewPlan:
    ready = {(group.face_id, group.region) for group in groups if not group.reasons}
    faces: dict[str, set[str]] = defaultdict(set)
    for record in preview.snapshot.records.values():
        if isinstance(record.data, FaceData):
            faces[record.data.card_id].add(record.data.id)
    blocked = frozenset(
        (p.data.card_id, p.data.region)
        for p in preview.included("printing")
        if isinstance(p.data, PrintingData)
        and any((face, p.data.region) not in ready for face in faces[p.data.card_id])
    )
    return close_preview(preview, blocked)


def plan_text_observations(
    preview: PreviewPlan, provider: TextProvider, *, images: ImageProvider | None = None
) -> TextPlan:
    """Read every selected-region physical observation before any current selection."""
    observations: list[FaceObservation] = []
    unavailable = []
    for _, record in sorted(preview.snapshot.records.items()):
        data = record.data
        if not isinstance(data, PrintingData) or data.region not in preview.regions:
            continue
        card = provider.card(data.region, data.card_no)
        if card is None:
            unavailable.append(data.id)
            continue
        verify_card(card)
        evidence = preview.evidence.get((data.region, data.card_no))
        if (
            evidence is None
            or source_values(evidence.source) != source_values(card.source)
            or evidence.observation != card.observation
        ):
            raise ValueError(
                "Text source disagrees with independently verified identity evidence"
            )
        if (card.observation.region, card.observation.card_no) != (
            data.region,
            data.card_no,
        ):
            raise ValueError("Text source region/card number mismatch")
        if {mapping.source_index for mapping in data.source_face_map} != set(
            range(len(card.faces))
        ):
            raise ValueError("Text source face map is incomplete")
        observations.extend(
            FaceObservation(
                card_id=data.card_id,
                printing_id=data.id,
                face_id=mapping.face_id,
                region=data.region,
                card_no=data.card_no,
                source_index=mapping.source_index,
                card=card,
                content=card.projected(mapping.source_index),
            )
            for mapping in data.source_face_map
        )
    exact = tuple(
        sorted(
            observations,
            key=lambda item: (
                item.face_id,
                item.region,
                item.printing_id,
                item.card.source.id,
            ),
        )
    )
    applications = None if images is None else plan_applications(preview, exact, images)
    groups = _groups(
        preview,
        corrected_observations(exact, applications or ()),
        tuple(sorted(unavailable)),
        applications,
    )
    return TextPlan(
        preview,
        exact,
        groups,
        tuple(sorted(unavailable)),
        _diagnostic_exclusions(preview, groups),
        applications,
    )
