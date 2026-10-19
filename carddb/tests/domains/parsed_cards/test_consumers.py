"""Exercise downstream consumers after their entire HTML archive is removed."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import digest
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.catalog.adoption_models import Batch, ReviewContext, SourceRef
from sve_carddb.domains.catalog.adoption_sources import AdoptionSources
from sve_carddb.domains.digital.links.evidence import batch_refs
from sve_carddb.domains.parsed_cards.generate import generate
from sve_carddb.domains.parsed_cards.sources import ParsedBatches
from sve_carddb.domains.products.evidence import resolve_evidence
from sve_carddb.domains.products.identities import load_product_identities
from sve_carddb.domains.products.models import Evidence
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.ingest.archive.frozen_sources import FrozenSources

from ...support.product_identity_fixtures import identity_fixture as identity_fixture  # ruff: ignore[useless-import-alias] -- shared fixture
from .test_inputs import REVISION, _batch

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.regions import Region

    from ...support.product_identity_fixtures import IdentityFixture


@pytest.mark.parametrize("region", ["en", "jp"])
def test_adopted_text_and_product_evidence_without_html(
    tmp_path: Path, region: Region
) -> None:
    sources, number = _batch(tmp_path, region)
    catalog = generate(
        [(region, sources)], {(region, number): "SYN"}, producer_revision=REVISION
    )
    version, observation = next(iter(catalog.observations().items()))
    name = observation.document.record.faces[0].name
    ref = SourceRef(
        batch_id=sources.batch_id,
        source_version_id=version,
        parser=f"official-{region}-exact-v1",
        locator="/faces/0/name",
        text_hash=digest(name.encode()),
    )
    review = ReviewContext(
        context=BuildContext.from_inputs(REVISION, {}),
        source_batches=(Batch(batch_id=sources.batch_id),),
    )
    stores = {sources.store_id: sources.root}
    old = AdoptionSources(stores, tmp_path)
    expected = old.text(ref, review)
    evidence = Evidence(
        batch_id=sources.batch_id,
        source_version_id=version,
        locator="/",
        role="synthetic",
    )
    expected_evidence = resolve_evidence((evidence,), stores)
    sources.root.rename(tmp_path / "unmounted")
    batches = ParsedBatches(catalog)
    new = AdoptionSources(stores, tmp_path, batches=batches)
    assert new.text(ref, review) == expected
    assert new.uses == old.uses
    assert resolve_evidence((evidence,), stores, batches=batches) == expected_evidence
    assert (
        batch_refs(
            Sources(stores, tmp_path, review.context, batches=batches),
            review.source_batches,
            "sv1",
        )
        == ()
    )
    assert (
        batch_refs(
            Sources(stores, tmp_path, review.context, batches=batches),
            review.source_batches,
            "svwb",
        )
        == ()
    )


def test_product_identity_matches_without_html(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    frozen = FrozenSources(fixture.store, "test-store", fixture.batch)
    catalog = generate(
        [("jp", frozen)],
        {("jp", page.card_no): "TEST" for page in fixture.pages},
        producer_revision=REVISION,
    )
    expected = fixture.load()
    fixture.store.rename(fixture.store.with_name("unmounted"))
    actual = load_product_identities(
        fixture.root,
        authored_revision=fixture.revision,
        catalog=fixture.catalog,
        stores={"test-store": fixture.store},
        batches=ParsedBatches(catalog),
    )
    assert actual == expected
