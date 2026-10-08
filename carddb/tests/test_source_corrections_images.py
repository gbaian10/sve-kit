"""Image evidence must remain exact, regional, archived and independently verifiable."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.source_corrections import FrozenImages
from sve_carddb.domains.text_observations import plan_text_observations
from sve_carddb.ingest.archive.source_archive import ArchiveError

if TYPE_CHECKING:
    from pathlib import Path

    from .shared_case_fixtures import CorrectionCaseTemplate


class TestDefaultCorrectionInputs:
    @pytest.mark.parametrize("field", ["sha256", "image_src", "region"])
    def test_each_image_lookup_pin_is_independently_required(
        self,
        tmp_path: Path,
        default_correction_case: CorrectionCaseTemplate,
        field: str,
    ) -> None:
        fixture = default_correction_case.copy(tmp_path)
        assert fixture.texts.plan.corrections is not None
        evidence = fixture.texts.plan.corrections[0].data.evidence[0]
        changed = evidence.model_copy(
            update={
                field: "sha256:" + "0" * 64
                if field == "sha256"
                else "/wrong.png"
                if field == "image_src"
                else "en"
            }
        )
        with pytest.raises(ValueError, match="absent"):
            fixture.images.image(changed)

    @pytest.mark.parametrize("part", ["raw", "descriptor", "receipt"])
    def test_source_closure_tampering_after_provider_creation_fails(
        self, tmp_path: Path, default_correction_case: CorrectionCaseTemplate, part: str
    ) -> None:
        fixture = default_correction_case.copy(tmp_path)
        sources = fixture.images.sources
        entry = sources.inventory.entries[0]
        descriptor = sources.descriptor(entry.source_version_id)
        relative = (
            entry.blob.path
            if part == "raw"
            else "descriptors/"
            + entry.descriptor_sha256.removeprefix("sha256:")
            + ".json"
            if part == "descriptor"
            else "receipts/"
            + descriptor.first_receipt_id.removeprefix("sha256:")
            + ".json"
        )
        (fixture.image_store / relative).write_bytes(b"Changed synthetic frozen bytes")
        with pytest.raises(ArchiveError, match="hash"):
            plan_text_observations(
                fixture.texts.identity, fixture.texts.provider, images=fixture.images
            )

    def test_card_html_batch_cannot_substitute_for_image_evidence(
        self, tmp_path: Path, default_correction_case: CorrectionCaseTemplate
    ) -> None:
        fixture = default_correction_case.copy(tmp_path)
        source = fixture.texts.plan.observations[0].card.source
        wrong = FrozenImages(
            fixture.texts.store, source.archive.store_id, source.archive.batch_id
        )
        with pytest.raises(ValueError, match="absent"):
            plan_text_observations(
                fixture.texts.identity, fixture.texts.provider, images=wrong
            )
