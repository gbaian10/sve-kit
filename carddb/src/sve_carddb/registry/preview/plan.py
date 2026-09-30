"""Regional projection after complete registry validation, without rewriting history."""

from collections import defaultdict
from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.build_inputs import SourceUse
from sve_carddb.registry.records import (
    AllocationData,
    ArtData,
    CardData,
    CorrectionData,
    EnglishPrintingData,
    FaceData,
    MappingReviewData,
    Observation,
    PrintingData,
    Region,
    RelatedData,
)
from sve_carddb.registry.snapshot import RegistryRecord, RegistrySnapshot, load_registry
from sve_carddb.snapshot.values import canonical

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.registry.preview.evidence import CardEvidence, EvidenceProvider


@dataclass(frozen=True)
class EvidenceCheck:
    region: Region
    card_no: str
    observation_hash: str
    rules_hash: str
    status: Literal["matched", "missing_source", "observation_mismatch"]
    source_id: str | None
    source_url: str | None
    raw_hash: str | None


@dataclass(frozen=True)
class Projection:
    record_key: str
    decision_id: str | None
    decision_state: str | None
    disposition: Literal["included", "excluded", "deferred"]
    reasons: tuple[str, ...]
    evidence: tuple[EvidenceCheck, ...]
    regions: tuple[Region, ...]


@dataclass(frozen=True)
class PreviewPlan:
    snapshot: RegistrySnapshot
    regions: tuple[Region, ...]
    projections: tuple[Projection, ...]
    evidence: Mapping[tuple[Region, str], CardEvidence]

    def source_uses(self) -> tuple[SourceUse, ...]:
        """Declare all successfully read observations, including excluded identity evidence."""
        return tuple(
            SourceUse(
                source=item.source,
                usage="registry_observation",
                locator=canonical({"region": region, "card_no": number}).decode(),
            )
            for (region, number), item in sorted(self.evidence.items())
        )

    def included(self, kind: str) -> tuple[RegistryRecord, ...]:
        """Return only records whose regional dependencies are included."""
        return tuple(
            self.snapshot.records[item.record_key]
            for item in self.projections
            if item.disposition == "included"
            and self.snapshot.records[item.record_key].kind == kind
        )

    def report(self) -> dict[str, JsonValue]:
        """Contain identifiers and hashes only; never official card text or corrections."""
        return {
            "regions": list[JsonValue](self.regions),
            "next_int_id": dict(self.snapshot.files.index().next_int_id),
            "records": [
                {
                    "record_key": item.record_key,
                    "decision_id": item.decision_id,
                    "historic_decision_state": item.decision_state,
                    "disposition": item.disposition,
                    "reasons": list[JsonValue](item.reasons),
                    "regions": list[JsonValue](item.regions),
                    "evidence": [
                        {
                            "region": check.region,
                            "card_no": check.card_no,
                            "recipe": "registry-observation-v1",
                            "observation_hash": check.observation_hash,
                            "rules_hash": check.rules_hash,
                            "status": check.status,
                            "source_id": check.source_id,
                            "source_url": check.source_url,
                            "raw_hash": check.raw_hash,
                        }
                        for check in item.evidence
                    ],
                }
                for item in self.projections
            ],
        }


class _Evidence:
    def __init__(self, provider: EvidenceProvider) -> None:
        self.provider = provider
        self.cards: dict[tuple[Region, str], CardEvidence] = {}
        self.checks: dict[Observation, EvidenceCheck] = {}

    def check(self, expected: Observation) -> EvidenceCheck:
        # Related evidence carries a role, which is not part of the observation recipe.
        expected = Observation.model_validate(expected.model_dump(exclude={"role"}))
        if expected in self.checks:
            return self.checks[expected]
        key = expected.region, expected.card_no
        found = self.provider.card(*key)
        status: Literal["matched", "missing_source", "observation_mismatch"] = (
            "missing_source"
        )
        if found is not None:
            previous = self.cards.setdefault(key, found)
            if previous != found:
                raise ValueError("Evidence provider changed a pinned source")
            status = (
                "matched" if found.observation == expected else "observation_mismatch"
            )
        check = EvidenceCheck(
            *key,
            expected.observation_hash,
            expected.rules_hash,
            status,
            found.source.id if found else None,
            found.source.url if found else None,
            found.source.sha256 if found else None,
        )
        self.checks[expected] = check
        return check


def _observations(data: PrintingData) -> tuple[Observation, ...]:
    if isinstance(data, EnglishPrintingData):
        target = data.cross_region_review.target_observation
        if target is not None:
            return data.observation, target
    return (data.observation,)


def plan_preview(
    authored: Path, provider: EvidenceProvider, *, regions: tuple[Region, ...]
) -> PreviewPlan:
    """Validate all indexed data first, then explicitly select output regions."""
    snapshot = load_registry(authored)
    if (
        not regions
        or len(set(regions)) != len(regions)
        or not set(regions) <= {"jp", "en"}
    ):
        raise ValueError("Explicit unique output regions are required")
    return _project(snapshot, provider, tuple(sorted(regions)))


class _Projector:
    def __init__(
        self,
        snapshot: RegistrySnapshot,
        provider: EvidenceProvider,
        regions: tuple[Region, ...],
    ) -> None:
        self.snapshot = snapshot
        self.evidence = _Evidence(provider)
        self.regions = regions
        self.printings = {
            item.data.id: item.data
            for item in snapshot.records.values()
            if isinstance(item.data, PrintingData)
        }
        self.checks = {
            p.id: tuple(self.evidence.check(obs) for obs in _observations(p))
            for p in self.printings.values()
        }
        self.selected: set[str] = set()
        self.card_regions: dict[str, set[Region]] = defaultdict(set)
        self.card_checks: dict[str, list[EvidenceCheck]] = defaultdict(list)
        self.printing_reasons: dict[str, tuple[str, ...]] = {}
        for printing in self.printings.values():
            self.card_checks[printing.card_id].extend(self.checks[printing.id])
            reasons = self._printing_reasons(printing)
            self.printing_reasons[printing.id] = reasons
            if not reasons:
                self.selected.add(printing.id)
                self.card_regions[printing.card_id].add(printing.region)
                source = self.evidence.cards[printing.region, printing.card_no]
                if {m.source_index for m in printing.source_face_map} != set(
                    range(len(source.faces))
                ):
                    raise ValueError("Source face map does not cover extracted faces")

    def _confirmed(self, key: str) -> bool:
        decision = self.snapshot.records[key].decision_id
        return (
            decision is not None
            and self.snapshot.decisions[decision].state == "confirmed"
        )

    def _printing_reasons(self, data: PrintingData) -> tuple[str, ...]:
        reasons: list[str] = []
        if data.region not in self.regions:
            reasons.append("outside_output_regions")
        reasons.extend(
            sorted({c.status for c in self.checks[data.id] if c.status != "matched"})
        )
        keys = ["printing:" + data.id, "card:" + data.card_id]
        keys.extend("face:" + m.face_id for m in data.source_face_map)
        if not all(self._confirmed(key) for key in keys):
            reasons.append("identity_decision_not_confirmed")
        return tuple(reasons)

    def _art(
        self, data: ArtData
    ) -> tuple[tuple[str, ...], tuple[EvidenceCheck, ...], tuple[Region, ...]]:
        used = tuple(
            self.evidence.check(self.printings[use.printing_id].observation)
            for use in data.uses
        )
        reasons: list[str] = []
        regions = tuple(
            sorted(
                {
                    self.printings[use.printing_id].region
                    for use in data.uses
                    if use.printing_id in self.selected
                }
            )
        )
        if not regions:
            reasons.append("no_included_use")
        if any(c.status != "matched" for c in used):
            reasons.append("art_evidence_unavailable")
        if data.classification != "unclassified":
            reasons.append("art_baseline_deferred")
        return tuple(reasons), used, regions

    def _mapping(
        self, data: MappingReviewData
    ) -> tuple[tuple[str, ...], tuple[EvidenceCheck, ...], tuple[Region, ...]]:
        used = tuple(self.evidence.check(obs) for obs in data.observations)
        reasons: list[str] = []
        if data.card_id not in self.card_regions:
            reasons.append("no_included_printing")
        if any(c.status != "matched" for c in used):
            reasons.append("mapping_evidence_unavailable")
        if not self.evidence.provider.coverage(data.coverage_hash):
            reasons.append("missing_review_coverage")
        return (
            tuple(reasons),
            used,
            tuple(sorted(self.card_regions.get(data.card_id, ()))),
        )

    def _related(
        self, data: RelatedData
    ) -> tuple[tuple[str, ...], tuple[EvidenceCheck, ...], tuple[Region, ...]]:
        used = tuple(self.evidence.check(obs) for obs in data.evidence)
        allowed = tuple(
            region
            for region in self.regions
            if all(
                region in self.card_regions.get(cid, ())
                for cid in (data.from_card_id, data.to_card_id)
            )
            and all(c.status == "matched" for c in used if c.region == region)
        )
        return (
            (() if allowed else ("no_region_with_both_verified_endpoints",)),
            used,
            allowed,
        )

    def _record(
        self, record: RegistryRecord
    ) -> tuple[tuple[str, ...], tuple[EvidenceCheck, ...], tuple[Region, ...]]:
        data = record.data
        if isinstance(data, PrintingData):
            return self.printing_reasons[data.id], self.checks[data.id], (data.region,)
        if isinstance(data, (CardData, FaceData)):
            cid = data.id if isinstance(data, CardData) else data.card_id
            return (
                () if cid in self.card_regions else ("no_included_printing",),
                tuple(self.card_checks[cid]),
                tuple(sorted(self.card_regions.get(cid, ()))),
            )
        if isinstance(data, AllocationData):
            return (
                (() if data.printing_id in self.selected else ("printing_excluded",)),
                (),
                (self.printings[data.printing_id].region,),
            )
        if isinstance(data, CorrectionData):
            return ("source_correction_deferred",), (), ()
        return self._review(record)

    def _review(
        self, record: RegistryRecord
    ) -> tuple[tuple[str, ...], tuple[EvidenceCheck, ...], tuple[Region, ...]]:
        data = record.data
        if isinstance(data, ArtData):
            return self._art(data)
        if isinstance(data, MappingReviewData):
            return self._mapping(data)
        if isinstance(data, RelatedData):
            return self._related(data)
        raise TypeError("Unsupported registry record kind")

    def projection(self, record: RegistryRecord) -> Projection:
        """Keep historical decisions even when current source evidence is unavailable."""
        reasons, used, regions = self._record(record)
        state = (
            self.snapshot.decisions[record.decision_id].state
            if record.decision_id
            else None
        )
        if (
            not isinstance(record.data, (AllocationData, CorrectionData))
            and state != "confirmed"
        ):
            reasons += ("decision_not_confirmed",)
        return Projection(
            record.record_key,
            record.decision_id,
            state,
            "deferred"
            if isinstance(record.data, CorrectionData)
            else "excluded"
            if reasons
            else "included",
            reasons,
            used,
            regions if not reasons else (),
        )


def _project(
    snapshot: RegistrySnapshot, provider: EvidenceProvider, regions: tuple[Region, ...]
) -> PreviewPlan:
    projector = _Projector(snapshot, provider, regions)
    projections = tuple(
        projector.projection(record) for _, record in sorted(snapshot.records.items())
    )
    return PreviewPlan(
        snapshot, regions, projections, MappingProxyType(projector.evidence.cards)
    )
