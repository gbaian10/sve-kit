"""Independent synthetic image bytes, authored corrections and typed text parents."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from sve_carddb.manifest import Region as SourceRegion
from sve_carddb.registry.records import CorrectionData
from sve_carddb.registry.review import Correction
from sve_carddb.snapshot.values import digest
from sve_carddb.source_archive import seal_batch
from sve_carddb.source_corrections import FrozenImages
from sve_carddb.source_corrections.images import evidence_url
from sve_carddb.text_observations import Binding, Vocabulary, plan_text_observations

from .test_source_archive import _put, _resource, _store
from .text_observation_fixtures import make_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.records import Region
    from sve_carddb.registry.review import Inputs

    from .text_observation_fixtures import Case

RAW_IMAGE = b"\x89PNG\r\n\x1a\nsynthetic correction evidence"


@dataclass
class CorrectionCase:
    texts: Case
    images: FrozenImages
    image_store: Path


def make_correction_case(  # ruff: ignore[too-many-arguments] -- explicit before/after fields keep missing-type evidence independent of its corrected value
    root: Path,
    inputs: Inputs,
    *,
    region: Region = "jp",
    field: str = "effect",
    state: str = "active",
    corrected: str | None = None,
    raw_type: str = "Spell",
) -> CorrectionCase:
    number = "BP02-071" if region == "jp" else "BP02-070EN"
    original = (inputs.jp if region == "jp" else inputs.en)[number].faces[0]
    if region == "en" and field == "card_type":
        original.info["Card Type"] = raw_type
    expected = (
        original.text
        if field == "effect"
        else original.card_type
        if region == "jp"
        else original.info["Card Type"]
    )
    assert expected is not None
    value = (
        corrected
        if corrected is not None
        else "Rule. (Reminder.)"
        if field == "effect"
        else "Follower"
    )
    inputs.receipt.corrections = [
        Correction(
            region=region,
            card_no=number,
            field=field,
            expected_raw_value=expected,
            corrected_value=value,
            image_sha256=digest(RAW_IMAGE),
            locator="Synthetic field box",
            state=state,
            reason="Synthetic source transcription correction",
        )
    ]
    case = make_case(root / "authored", inputs)
    store = replace(_store(root / "images"), store_id="image-store")
    for record in case.identity.snapshot.records.values():
        if isinstance(record.data, CorrectionData):
            for evidence in record.data.evidence:
                _put(
                    store,
                    replace(
                        _resource(
                            evidence_url(evidence), "images/evidence.png", RAW_IMAGE
                        ),
                        region=SourceRegion(evidence.region),
                    ),
                    RAW_IMAGE,
                )
    sealed = seal_batch(store)
    images = FrozenImages(store.root, store.store_id, sealed.batch_id)
    if field == "card_type":
        case.vocabulary = Vocabulary(
            bindings=(
                *case.vocabulary.bindings,
                Binding(region=region, kind="type", raw="Spell", code="spell"),
            )
        )
    case.plan = plan_text_observations(case.identity, case.provider, images=images)
    return CorrectionCase(case, images, store.root)
