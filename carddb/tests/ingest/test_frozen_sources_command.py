"""Shared command readers retain verified projections and reject mismatched input pins."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.card_extras import FrozenCardExtras
from sve_carddb.domains.products import FrozenProducts
from sve_carddb.domains.text_observations import FrozenTexts
from sve_carddb.ingest.archive.frozen_sources import FrozenBatches
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import ArchiveError, Scope, seal_batch
from sve_carddb.parse.pages import official_en, official_jp

from ..domains.text_observations.test_effect_presence import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.ingest.archive.frozen_sources import FrozenSources
    from sve_carddb.ingest.archive.source_archive import ArchiveStore


@pytest.fixture
def command_sources(
    tmp_path: Path,
) -> tuple[ArchiveStore, FrozenBatches, FrozenSources]:
    store = _store(tmp_path / "sources")
    for region in (SourceRegion.EN, SourceRegion.JP):
        url = (official_en if region == SourceRegion.EN else official_jp).card_url(
            "SYN-01"
        )
        raw = page("en" if region == SourceRegion.EN else "jp")
        _put(
            store,
            replace(
                _resource(url, f"raw/{region.value}.html", raw, Kind.CARD),
                region=region,
            ),
            raw,
        )
    batch = seal_batch(store, scope=(Scope(provider="en", kind="card"),))
    readers = FrozenBatches()
    return store, readers, readers.batch(store.root, store.store_id, batch.batch_id)


@pytest.mark.parametrize("changed", ["descriptor", "receipt", "raw"])
@pytest.mark.parametrize("adapter", ["texts", "products"])
def test_shared_adapters_reuse_verified_sources_within_one_command(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources],
    changed: str,
    adapter: str,
) -> None:
    store, readers, sources = command_sources
    texts = FrozenTexts(
        store.root,
        store.store_id,
        sources.batch_id,
        region="en",
        parser_version="official-en-exact-v1",
        sources=readers.configured({store.store_id: store.root}, sources.batch_id),
    )
    products = FrozenProducts(
        store.root, store.store_id, sources.batch_id, region="en", sources=sources
    )
    original = texts.card("en", "SYN-01")
    original_products = products.pages()
    assert original is not None
    assert original.raw is None
    assert len(original_products) == 1
    entry = sources.inventory.entries[0]
    if changed == "descriptor":
        target = store.root / "descriptors" / (entry.descriptor_sha256[7:] + ".json")
    elif changed == "receipt":
        descriptor = sources.descriptor(entry.source_version_id)
        target = store.root / "receipts" / (descriptor.first_receipt_id[7:] + ".json")
    else:
        target = store.root / entry.blob.path
    target.write_bytes(b"Synthetic changed source")
    read = (
        (lambda: texts.card("en", "SYN-01")) if adapter == "texts" else products.pages
    )
    assert read() == (original if adapter == "texts" else original_products)
    with pytest.raises(ArchiveError):
        FrozenBatches().batch(store.root, store.store_id, sources.batch_id)


def test_shared_reader_rejects_version_from_another_batch(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources],
) -> None:
    store, readers, en = command_sources
    jp_batch = seal_batch(store, scope=(Scope(provider="jp", kind="card"),))
    jp = readers.batch(store.root, store.store_id, jp_batch.batch_id)
    with pytest.raises(ArchiveError, match="absent from pinned batch"):
        en.read(jp.inventory.entries[0].source_version_id, parser_version="synthetic")


@pytest.mark.parametrize("mismatch", ["root", "store", "batch"])
def test_adapter_cannot_borrow_reader_from_another_input_pin(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources], mismatch: str
) -> None:
    store, _, sources = command_sources
    with pytest.raises(ValueError, match="differs from configured input"):
        FrozenTexts(
            store.root / "other" if mismatch == "root" else store.root,
            "other-store" if mismatch == "store" else store.store_id,
            "sha256:" + "0" * 64 if mismatch == "batch" else sources.batch_id,
            region="en",
            parser_version="official-en-exact-v1",
            sources=sources,
        )


def test_opened_batch_does_not_authorize_a_different_configured_store(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources],
) -> None:
    store, readers, sources = command_sources
    with pytest.raises(ArchiveError, match="invalid archive blob locator"):
        readers.configured({"other-store": store.root}, sources.batch_id)


def test_first_consumption_rejects_raw_changed_after_batch_verification(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources],
) -> None:
    store, _, sources = command_sources
    entry = sources.inventory.entries[0]
    (store.root / entry.blob.path).write_bytes(b"Synthetic damaged raw")
    with pytest.raises(ArchiveError, match="content hash mismatch"):
        FrozenTexts(
            store.root,
            store.store_id,
            sources.batch_id,
            region="en",
            parser_version="official-en-exact-v1",
            sources=sources,
        ).card("en", "SYN-01")


def test_adapters_share_page_projections_without_retaining_raw(
    command_sources: tuple[ArchiveStore, FrozenBatches, FrozenSources],
) -> None:
    store, _, sources = command_sources
    products = FrozenProducts(
        store.root,
        store.store_id,
        sources.batch_id,
        region="en",
        sources=sources,
    ).pages()
    entry = sources.inventory.entries[0]
    (store.root / entry.blob.path).unlink()
    texts = FrozenTexts(
        store.root,
        store.store_id,
        sources.batch_id,
        region="en",
        parser_version="alternate-provenance-pin",
        sources=sources,
    ).card("en", "SYN-01")
    assert texts is not None
    assert texts.raw is None
    assert texts.source.parser_version == "alternate-provenance-pin"
    supplements = tuple(
        FrozenCardExtras(
            store.root,
            store.store_id,
            sources.batch_id,
            region="en",
            sources=sources,
        ).pages()
    )
    assert products[0].card_no == texts.observation.card_no == supplements[0].card_no
    assert products[0].source.parser_version == "official-product-block-v1"
    assert supplements[0].source.parser_version == "official-card-extras-en-v1"
