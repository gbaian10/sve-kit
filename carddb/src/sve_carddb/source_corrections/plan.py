"""Pin corrections separately from raw observations and compare corrected candidates."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import SourceUse
from sve_carddb.registry.corrections import Status, correction_status
from sve_carddb.registry.records import CorrectionData, PrintingData, Region
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.source_corrections.images import evidence_url

if TYPE_CHECKING:
    from sve_carddb.build_inputs import Source
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.registry.snapshot import RegistryRecord
    from sve_carddb.source_corrections.images import ImageProvider
    from sve_carddb.text_observations.models import FaceObservation


@dataclass(frozen=True)
class Application:
    record: RegistryRecord
    observation: FaceObservation
    images: tuple[Source, ...]
    status: Status | None

    @property
    def data(self) -> CorrectionData:
        """Expose only the validated authored correction model."""
        assert isinstance(self.record.data, CorrectionData)
        return self.record.data

    def key(self) -> str:
        """Invalidate candidate/review identity on every authored correction change."""
        return digest(self.record.content)

    def uses(self) -> tuple[SourceUse, ...]:
        """Pin both the observation comparison and every exact image locator."""
        return (
            SourceUse(
                source=self.observation.card.source,
                usage="source_correction_comparison",
                locator=canonical(
                    {
                        "correction_id": self.data.id,
                        "observation": self.observation.locator(),
                    }
                ).decode(),
            ),
            *(
                SourceUse(
                    source=source,
                    usage="source_correction_evidence",
                    locator=canonical(
                        {
                            "correction_id": self.data.id,
                            "evidence": evidence.model_dump(mode="json"),
                        }
                    ).decode(),
                )
                for evidence, source in zip(
                    self.data.evidence, self.images, strict=True
                )
            ),
        )

    def marker(self, source_url: str | None) -> dict[str, JsonValue] | None:
        """Keep provenance on this successfully changed reference, never on text."""
        if self.status != "applied":
            return None
        return {
            "field": self.data.field,
            "corrected_from": self.data.expected_raw_value,
            "is_corrected": True,
            "reason": self.data.reason,
            "source_url": source_url,
        }

    def report(self) -> dict[str, JsonValue]:
        """Report identifiers, exact hashes and statuses without any official text."""
        return {
            "correction_id": self.data.id,
            "printing_id": self.data.printing_id,
            "face_id": self.data.face_id,
            "field": self.data.field,
            "status": self.status or "needs_review",
            "correction_hash": self.key(),
            "raw_face_hash": self.observation.content.fingerprint(),
            "observation_hash": self.observation.card.observation.observation_hash,
            "image_source_ids": [source.id for source in self.images],
            "warning": "retire_upstream_fixed_correction"
            if self.status == "already_fixed"
            else None,
        }


def _status(data: CorrectionData, item: FaceObservation) -> Status | None:
    if data.state != "active":
        return None
    return correction_status(
        item.content.effect if data.field == "effect" else item.content.type_raw,
        item.card.observation.observation_hash,
        expected=data.expected_raw_value,
        corrected=data.corrected_value,
        expected_hash=data.expected_source_hash,
    )


def _verify_application(preview: PreviewPlan, application: Application) -> None:
    data = application.data
    item = application.observation
    if (data.printing_id, data.face_id) != (item.printing_id, item.face_id):
        raise ValueError("Correction observation scope mismatch")
    record = preview.snapshot.records.get(application.record.record_key)
    if record != application.record or application.status != _status(data, item):
        raise ValueError("Correction record or application status mismatch")
    if (
        CorrectionData.model_validate_json(canonical(application.record.entry().data))
        != data
    ):
        raise ValueError("Correction typed data differs from its exact authored record")
    decision = preview.snapshot.decisions.get(application.record.decision_id or "")
    if decision is None or (data.state == "active" and decision.state != "confirmed"):
        raise ValueError("Correction adoption decision is not confirmed")
    if (application.record.record_key, application.key()) not in decision.members:
        raise ValueError("Correction decision does not bind the exact record")
    if len(data.evidence) != len(application.images):
        raise ValueError("Correction image evidence inventory mismatch")
    for evidence, source in zip(data.evidence, application.images, strict=True):
        if evidence.region != item.region or (
            source.kind,
            source.url,
            source.sha256,
        ) != ("image", evidence_url(evidence), evidence.sha256):
            raise ValueError("Correction image evidence metadata mismatch")


def selected_records(preview: PreviewPlan) -> tuple[RegistryRecord, ...]:
    """Preserve exact complete envelopes while importing only included regional parents."""
    included = {
        record.data.id
        for record in preview.included("printing")
        if isinstance(record.data, PrintingData)
    }
    return tuple(
        record
        for _, record in sorted(preview.snapshot.records.items())
        if isinstance(record.data, CorrectionData)
        and record.data.printing_id in included
    )


def withheld_regions(preview: PreviewPlan) -> frozenset[tuple[str, Region]]:
    """Prevent excluded active correction parents from leaving sibling output open."""
    included = {record.record_key for record in selected_records(preview)}
    blocked = set[tuple[str, Region]]()
    for record in preview.snapshot.records.values():
        data = record.data
        if (
            isinstance(data, CorrectionData)
            and data.state == "active"
            and record.record_key not in included
        ):
            printing = preview.snapshot.records["printing:" + data.printing_id].data
            if (
                isinstance(printing, PrintingData)
                and printing.region in preview.regions
            ):
                blocked.add((printing.card_id, printing.region))
    return frozenset(blocked)


def plan_applications(
    preview: PreviewPlan,
    observations: tuple[FaceObservation, ...],
    images: ImageProvider,
) -> tuple[Application, ...]:
    """Require every scoped source/evidence; do not silently skip unavailable inputs."""
    by_use = {(item.printing_id, item.face_id): item for item in observations}
    result = []
    for record in selected_records(preview):
        data = record.data
        assert isinstance(data, CorrectionData)
        item = by_use.get((data.printing_id, data.face_id))
        if item is None:
            raise ValueError("Correction requires its raw face observation")
        application = Application(
            record,
            item,
            tuple(images.image(e) for e in data.evidence),
            _status(data, item),
        )
        _verify_application(preview, application)
        result.append(application)
    return tuple(result)


def verify_applications(
    preview: PreviewPlan,
    observations: tuple[FaceObservation, ...],
    applications: tuple[Application, ...],
) -> None:
    """Reject omitted, extra, duplicated, altered or unbound correction uses."""
    if tuple(a.record for a in applications) != selected_records(preview):
        raise ValueError("Correction application inventory mismatch")
    by_use = {(item.printing_id, item.face_id): item for item in observations}
    fields: set[tuple[str, str, str]] = set()
    for application in applications:
        data = application.data
        key = data.printing_id, data.face_id, data.field
        if key in fields or application.observation != by_use.get(key[:2]):
            raise ValueError("Correction field/source inventory mismatch")
        fields.add(key)
        _verify_application(preview, application)


def corrected_observations(
    observations: tuple[FaceObservation, ...], applications: tuple[Application, ...]
) -> tuple[FaceObservation, ...]:
    """Apply successful field-local replacements before hashes and candidate comparison."""
    result = []
    for item in observations:
        applied = tuple(
            a for a in applications if a.observation == item and a.status == "applied"
        )
        fields = {
            "effect" if a.data.field == "effect" else "type_raw": a.data.corrected_value
            for a in applied
        }
        result.append(
            item.model_copy(
                update={
                    "content": item.content.model_copy(update=fields),
                    "correction_keys": tuple(a.key() for a in applied),
                }
            )
            if applied
            else item
        )
    return tuple(result)
