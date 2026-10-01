"""Image evidence must remain exact, regional, archived and independently verifiable."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.source_archive import ArchiveError
from sve_carddb.source_corrections import FrozenImages
from sve_carddb.text_observations import plan_text_observations

from .source_correction_fixtures import make_correction_case
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.review import Inputs


@pytest.mark.parametrize("field", ["sha256", "image_src", "region"])
def test_each_image_lookup_pin_is_independently_required(
    tmp_path: Path, inputs: Inputs, field: str
) -> None:
    fixture = make_correction_case(tmp_path, inputs)
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
    tmp_path: Path, inputs: Inputs, part: str
) -> None:
    fixture = make_correction_case(tmp_path, inputs)
    sources = fixture.images.sources
    entry = sources.inventory.entries[0]
    descriptor = sources.descriptor(entry.source_version_id)
    relative = (
        entry.blob.path
        if part == "raw"
        else "descriptors/" + entry.descriptor_sha256.removeprefix("sha256:") + ".json"
        if part == "descriptor"
        else "receipts/" + descriptor.first_receipt_id.removeprefix("sha256:") + ".json"
    )
    (fixture.image_store / relative).write_bytes(b"Changed synthetic frozen bytes")
    with pytest.raises(ArchiveError, match="hash"):
        plan_text_observations(
            fixture.texts.identity, fixture.texts.provider, images=fixture.images
        )


def test_card_html_batch_cannot_substitute_for_image_evidence(
    tmp_path: Path, inputs: Inputs
) -> None:
    fixture = make_correction_case(tmp_path, inputs)
    source = fixture.texts.plan.observations[0].card.source
    wrong = FrozenImages(
        fixture.texts.store, source.archive.store_id, source.archive.batch_id
    )
    with pytest.raises(ValueError, match="absent"):
        plan_text_observations(
            fixture.texts.identity, fixture.texts.provider, images=wrong
        )
